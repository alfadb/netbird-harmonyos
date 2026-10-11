//! Process-global RUST HEAP accounting (allocation counter + read-only view).
//!
//! `CountingAllocator` wraps `std::alloc::System` (on OHOS: the platform musl
//! `malloc`) with four relaxed atomics, and `lib.rs` installs it as this
//! crate's `#[global_allocator]`. The counters answer one question for the VPN
//! process: how much memory is the RUST heap holding right now, as opposed to
//! everything else that lives in the same process — the ArkTS/JS heap, NAPI
//! objects, libace_napi internals, C/C++ buffers of the platform TUN/route
//! code. The anonymous-memory growth of the process is the sum of all of
//! those; this module isolates one term of it.
//!
//! Counted: every `alloc` / `alloc_zeroed` / `realloc` / `dealloc` that Rust
//! routes through the global allocator — this crate's own allocations, those
//! of every statically linked Rust dependency (BoringTun's Rust side
//! included), and every host test binary that links this crate as an rlib.
//! NOT counted: memory the C/C++/ArkTS side allocates with its own `malloc`
//! (libace_napi, platform TUN/route/VpnConnection code), mmap'ed stacks, and
//! anything else that never enters Rust's allocator API. This is a Rust-heap
//! view, NEVER a process-RSS view.
//!
//! Semantics of the numbers (`AllocSnapshot`, all cumulative unless stated):
//! - `bytes`  — live bytes currently requested through this allocator:
//!              `alloc + alloc_zeroed + realloc delta - dealloc`. Never wraps
//!              (saturating subtraction): an accounting leak shows up as `0`,
//!              never as an absurdly large value.
//! - `peak`   — high-water mark of `bytes` since process start. A reader also
//!              gets `max(peak_atomic, bytes)`, so `peak < bytes` can never be
//!              observed even while a concurrent allocation's `fetch_max` is
//!              still in flight.
//! - `allocs` — allocation EVENTS: `alloc` + `alloc_zeroed` + one per
//!              successful `realloc` (a realloc is one allocate event and one
//!              free event for the same block, in whichever direction it
//!              moved; only `bytes` sees the size difference).
//! - `frees`  — deallocation EVENTS: `dealloc` + one per successful `realloc`.
//! A FAILED (null-returning) `alloc` / `alloc_zeroed` / `realloc` is counted
//! as nothing, and a failed realloc leaves the original block's bytes charged:
//! the old block is still live.
//!
//! Cost and concurrency: no lock, no syscall, no allocation in the accounting
//! path itself — 2-4 relaxed atomic read-modify-writes per allocator event
//! (`fetch_add` for the live counter, `fetch_max` when it grows, one
//! `fetch_add` per event counter). Readers (`snapshot`) never block writers
//! and vice versa; under multi-threaded allocation the two shared counter
//! cache lines are the only added contention.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

static CURRENT_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_BYTES: AtomicU64 = AtomicU64::new(0);
static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static FREE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Point-in-time copy of the four counters (see the module docs for what each
/// one means). `Copy`, so handing it around never allocates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocSnapshot {
    pub bytes: u64,
    pub peak: u64,
    pub allocs: u64,
    pub frees: u64,
}

/// Read the counters. Lock-free and allocation-free: safe to call from any
/// thread, including the NAPI callback thread.
pub fn snapshot() -> AllocSnapshot {
    // Read `bytes` first: the `peak` clamp below is then valid against it.
    let bytes = CURRENT_BYTES.load(Ordering::Relaxed);
    AllocSnapshot {
        bytes,
        peak: PEAK_BYTES.load(Ordering::Relaxed).max(bytes),
        allocs: ALLOC_COUNT.load(Ordering::Relaxed),
        frees: FREE_COUNT.load(Ordering::Relaxed),
    }
}

/// `snapshot()` as the frozen flat JSON document the NAPI export returns:
/// `{"bytes":n,"peak":n,"allocs":n,"frees":n}` (fixed key order, exactly four
/// numeric fields, no `ok`). The counters are read BEFORE the reply string is
/// formatted, so the small `String` this function allocates for the reply is
/// not part of the numbers it reports.
pub fn snapshot_json() -> String {
    let s = snapshot();
    format!(
        "{{{},{},{},{}}}",
        crate::util::jnum("bytes", s.bytes),
        crate::util::jnum("peak", s.peak),
        crate::util::jnum("allocs", s.allocs),
        crate::util::jnum("frees", s.frees),
    )
}

/// Live bytes grow by `n` (new allocation or realloc growth) and the peak is
/// lifted to the new value.
fn charge(n: u64) {
    if n == 0 {
        return;
    }
    let now = CURRENT_BYTES.fetch_add(n, Ordering::Relaxed) + n;
    PEAK_BYTES.fetch_max(now, Ordering::Relaxed);
}

/// Live bytes shrink by `n` (dealloc or realloc shrink), saturating at zero.
fn uncharge(n: u64) {
    if n == 0 {
        return;
    }
    // CAS loop rather than a bare `fetch_sub`: a (hypothetical) accounting bug
    // must not wrap `bytes` around to ~2^64, which would poison every later
    // reading. Relaxed on both sides — the counter is an observation, not a
    // synchronization primitive.
    let _ = CURRENT_BYTES.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
        Some(cur.saturating_sub(n))
    });
}

/// `GlobalAlloc` adapter: delegates every operation to `System` unchanged and
/// records it. Zero-sized, no state — the counters are the module statics.
pub struct CountingAllocator;

impl CountingAllocator {
    pub const fn new() -> Self {
        CountingAllocator
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            charge(layout.size() as u64);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc_zeroed(layout);
        if !ptr.is_null() {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            charge(layout.size() as u64);
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let out = System.realloc(ptr, layout, new_size);
        if !out.is_null() {
            // The block survives as ONE allocation event plus ONE free event;
            // only the live-byte counter moves, by the size difference.
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            FREE_COUNT.fetch_add(1, Ordering::Relaxed);
            let old_size = layout.size();
            if new_size > old_size {
                charge((new_size - old_size) as u64);
            } else if new_size < old_size {
                uncharge((old_size - new_size) as u64);
            }
        }
        // A failed realloc (null) changed nothing: the original block is still
        // allocated and still charged.
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        FREE_COUNT.fetch_add(1, Ordering::Relaxed);
        uncharge(layout.size() as u64);
    }
}
