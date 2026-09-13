// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # backoff — management reconnect backoff (N3-4)
//!
//! The reconnect policy of the upstream NetBird management client, in the
//! exact shape of the `cenkalti/backoff/v4` (v4.3.0) `ExponentialBackOff` it
//! is built on. Evidence, upstream commit
//! `791401060d2b95e5f51e3439c0649729132f571e`:
//!
//! - **Stream (Sync) retry backoff** — `shared/management/client/grpc.go:188-198`
//!   `defaultBackoff()`: `InitialInterval: 800ms`, `RandomizationFactor: 1`,
//!   `Multiplier: 1.7`, `MaxInterval: 10s`, `MaxElapsedTime: 3*30*24h`
//!   ("3 months" — never exhausts in practice; the loop is meant to run for
//!   the process lifetime, see grpc.go:271 "exiting the Management service
//!   connection retry loop due to the unrecoverable error" being an anomaly).
//!   [`ExponentialBackoff::upstream_stream_default`] pins these numbers.
//! - **Dial backoff** — `client/grpc/dialer.go:22-27` `Backoff()`:
//!   `backoff.NewExponentialBackOff()` defaults (v4.3.0 `exponential.go`
//!   `Default*` consts: 500ms / 0.5 / 1.5 / 60s) with only `MaxElapsedTime`
//!   overridden to 10s — a bounded one-shot connect budget.
//!   [`ExponentialBackoff::upstream_dial_default`] pins these numbers.
//! - **Jitter** — `RandomizationFactor` f: delay is drawn uniformly from
//!   `[current×(1−f), current×(1+f)]` (v4.3.0 `exponential.go`
//!   `getRandomValueFromInterval`: `min + random×(max−min+1)` in
//!   nanosecond-resolution floats; the `+1` gives the endpoints inclusive
//!   weight — reproduced verbatim below). With the stream preset's f=1 the
//!   delay therefore ranges over `[0, 2×current]`.
//! - **Interval growth** — `current ×= Multiplier`, capped at `MaxInterval`;
//!   once `current ≥ MaxInterval/Multiplier` the cap value is assigned
//!   directly (v4.3.0 `incrementCurrentInterval`). Both presets converge to
//!   `MaxInterval` (10s / 60s).
//! - **Stop condition** — when `elapsed + next > MaxElapsedTime` (and
//!   `MaxElapsedTime != 0`) the next call yields `Stop`
//!   (v4.3.0 `NextBackOff`), i.e. [`ExponentialBackoff::next_delay`] returns
//!   `None`. `MaxElapsedTime == 0` means "never stop".
//! - **Reset on success** — the upstream retry loop resets the backoff when
//!   the Sync stream is (re)established (`grpc.go:457` `backOff.Reset()`,
//!   with the rationale comment at L446-456) and before the first attempt
//!   (`client/grpc/retry.go:22`). [`ExponentialBackoff::reset`] is the same
//!   operation; [`crate::sync::SyncSession`] drives it at the same points.
//!
//! ## Testability contract
//!
//! No sleeping and no global state: randomness and time are injected via
//! [`Rng`] and [`Clock`]. Tests use a deterministic [`Rng`] (fixed sequence)
//! and a manually advanced [`Clock`] to assert exact delay sequences —
//! the production [`SyncSession`](crate::sync::SyncSession) wires
//! [`OsRandom`] + [`MonotonicClock`] and sleeps with tokio, but that path is
//! never exercised by parameter assertions.

use std::time::{Duration, Instant};

/// Uniform `[0, 1)` source (Go `rand.Float64()` at v4.3.0
/// `exponential.go NextBackOff`).
pub trait Rng {
    /// Next uniform sample in `[0, 1)`.
    fn next_uniform(&mut self) -> f64;
}

/// Monotonic time source (Go `backoff.Clock`; used only for the
/// `MaxElapsedTime` elapsed budget, never for sleeping).
pub trait Clock {
    fn now(&self) -> Instant;
}

/// OS-backed RNG. Backed by the `rand_core::OsRng` already in the offline
/// dependency closure (re-exported as `crypto_box::aead::rand_core` — no new
/// dependency); 8 random bytes → `f64` in `[0, 1)` with 53 bits of mantissa
/// (no rounding bias).
#[derive(Debug, Clone, Copy, Default)]
pub struct OsRandom;

impl Rng for OsRandom {
    fn next_uniform(&mut self) -> f64 {
        use crypto_box::aead::rand_core::RngCore;
        let mut bytes = [0u8; 8];
        crypto_box::aead::OsRng.fill_bytes(&mut bytes);
        // top 53 bits → uniform in [0, 1) with full f64 mantissa precision
        let bits = u64::from_be_bytes(bytes) >> 11;
        (bits as f64) / (1u64 << 53) as f64
    }
}

/// System monotonic clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct MonotonicClock;

impl Clock for MonotonicClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Upstream-shaped exponential backoff with jitter (see module docs).
///
/// Mirrors cenkalti/backoff v4.3.0 `ExponentialBackOff` state machine:
/// `current_interval` starts at `initial_interval`, each `next_delay` first
/// jitters the current interval, then grows it toward `max_interval`, then
/// applies the elapsed budget (`max_elapsed`).
#[derive(Debug, Clone)]
pub struct ExponentialBackoff {
    initial_interval: Duration,
    randomization_factor: f64,
    multiplier: f64,
    max_interval: Duration,
    /// `None` = never stop (Go `MaxElapsedTime == 0`).
    max_elapsed: Option<Duration>,
    current_interval: Duration,
    started_at: Instant,
}

impl ExponentialBackoff {
    /// Sync-stream reconnect preset: the exact parameters of upstream
    /// `shared/management/client/grpc.go:188-198 defaultBackoff()`.
    pub fn upstream_stream_default() -> Self {
        ExponentialBackoff::new(
            Duration::from_millis(800),
            1.0,
            1.7,
            Duration::from_secs(10),
            Some(Duration::from_secs(3 * 30 * 24 * 60 * 60)), // 3 months
        )
    }

    /// Dial preset: upstream `client/grpc/dialer.go:22-27 Backoff()` —
    /// cenkalti/backoff v4.3.0 defaults with `MaxElapsedTime = 10s`.
    pub fn upstream_dial_default() -> Self {
        ExponentialBackoff::new(
            Duration::from_millis(500),
            0.5,
            1.5,
            Duration::from_secs(60),
            Some(Duration::from_secs(10)),
        )
    }

    /// Fully explicit construction (tests / future callers). `max_elapsed`
    /// of `None` disables the elapsed budget (Go `MaxElapsedTime == 0`).
    pub fn new(
        initial_interval: Duration,
        randomization_factor: f64,
        multiplier: f64,
        max_interval: Duration,
        max_elapsed: Option<Duration>,
    ) -> Self {
        assert!(
            initial_interval > Duration::ZERO,
            "initial_interval must be positive"
        );
        assert!(multiplier >= 1.0, "multiplier must be >= 1");
        assert!(
            randomization_factor >= 0.0,
            "randomization_factor must be >= 0"
        );
        assert!(max_interval >= initial_interval, "max_interval must be >= initial_interval");
        let mut backoff = ExponentialBackoff {
            initial_interval,
            randomization_factor,
            multiplier,
            max_interval,
            max_elapsed,
            current_interval: initial_interval,
            started_at: Instant::now(),
        };
        backoff.reset(&MonotonicClock);
        backoff
    }

    /// Back to the initial interval, restart the elapsed budget
    /// (v4.3.0 `Reset`; upstream calls it on every successful stream
    /// establishment — grpc.go:457 — and before the first attempt,
    /// retry.go:22).
    pub fn reset(&mut self, clock: &dyn Clock) {
        self.current_interval = self.initial_interval;
        self.started_at = clock.now();
    }

    /// The interval the NEXT growth step starts from (exposed for tests /
    /// diagnostics — not part of the decision logic).
    pub fn current_interval(&self) -> Duration {
        self.current_interval
    }

    /// Next delay, or `None` for Go `backoff.Stop` (elapsed budget spent).
    ///
    /// Same order of operations as v4.3.0 `NextBackOff`:
    /// jitter(current) → grow(current) → elapsed budget check on the
    /// jittered value.
    pub fn next_delay(&mut self, clock: &dyn Clock, rng: &mut dyn Rng) -> Option<Duration> {
        let elapsed = clock.now().duration_since(self.started_at);
        let next = self.jittered(rng);
        self.grow();
        if let Some(max) = self.max_elapsed {
            if elapsed + next > max {
                return None;
            }
        }
        Some(next)
    }

    /// `getRandomValueFromInterval` (v4.3.0): delay ∈
    /// `[current×(1−f), current×(1+f)]`, drawn as
    /// `min + random×(max−min+1)` in nanosecond floats (endpoints inclusive
    /// — the `+1ns` term is upstream-verbatim). `f == 0` returns the bare
    /// interval (upstream keeps no randomness in that case).
    fn jittered(&self, rng: &mut dyn Rng) -> Duration {
        let current_nanos = self.current_interval.as_nanos() as f64;
        if self.randomization_factor == 0.0 {
            return self.current_interval;
        }
        let delta = self.randomization_factor * current_nanos;
        let min = current_nanos - delta;
        let max = current_nanos + delta;
        let random = rng.next_uniform();
        // clamp: f==1 makes min == 0; a tiny float excursion below 0 would
        // panic Duration::from_secs_f64 — upstream tolerates the same
        // marginally-negative values only because Go durations saturate.
        let picked = (min + random * (max - min + 1.0)).max(0.0);
        Duration::from_nanos(picked as u64)
    }

    /// `incrementCurrentInterval` (v4.3.0): `current ≥ max/multiplier` →
    /// assign the cap; else multiply.
    fn grow(&mut self) {
        if self.multiplier > 1.0
            && self.current_interval.as_nanos() as f64
                >= self.max_interval.as_nanos() as f64 / self.multiplier
        {
            self.current_interval = self.max_interval;
        } else {
            self.current_interval =
                Duration::from_nanos((self.current_interval.as_nanos() as f64 * self.multiplier)
                    as u64);
            if self.current_interval > self.max_interval {
                self.current_interval = self.max_interval;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// unit tests — deterministic sequences, injected clock/rng, no sleeping
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed sequence RNG: pops samples in order, panics when exhausted
    /// (a test that consumes more samples than declared is wrong).
    #[derive(Default)]
    struct FixedRng {
        samples: Vec<f64>,
        idx: usize,
    }

    impl FixedRng {
        fn new(samples: &[f64]) -> Self {
            FixedRng { samples: samples.to_vec(), idx: 0 }
        }
    }

    impl Rng for FixedRng {
        fn next_uniform(&mut self) -> f64 {
            let s = self.samples[self.idx];
            self.idx += 1;
            s
        }
    }

    /// Manually advanced clock.
    struct MockClock {
        now: Instant,
    }

    impl MockClock {
        fn new() -> Self {
            MockClock { now: Instant::now() }
        }
        fn advance(&mut self, by: Duration) {
            self.now += by;
        }
    }

    impl Clock for MockClock {
        fn now(&self) -> Instant {
            self.now
        }
    }

    /// Stream preset growth without jitter influence: f=0 variant of the
    /// upstream numbers (800ms, ×1.7, cap 10s). Growth is float-multiplied
    /// exactly like the Go lib (`time.Duration(float64(current) × 1.7)`,
    /// truncating); expected values below are derived with that same
    /// expression so a regression in the chain (not in float rounding) is
    /// what fails.
    #[test]
    fn stream_preset_growth_chain_matches_multiplier_and_cap() {
        let mut b = ExponentialBackoff::new(
            Duration::from_millis(800),
            0.0, // f=0 → bare intervals, growth isolated from jitter
            1.7,
            Duration::from_secs(10),
            None,
        );
        let mut rng = FixedRng::default();
        let clock = MockClock::new();
        let step = |prev_nanos: u64| Duration::from_nanos((prev_nanos as f64 * 1.7) as u64);
        let want0 = Duration::from_millis(800);
        let want1 = step(800_000_000);
        let want2 = step(want1.as_nanos() as u64);
        let want3 = step(want2.as_nanos() as u64);
        let want4 = step(want3.as_nanos() as u64);
        let expected = [want0, want1, want2, want3, want4, Duration::from_secs(10)];
        for (i, want) in expected.iter().enumerate() {
            let got = b.next_delay(&clock, &mut rng).expect("not stopped");
            assert_eq!(&got, want, "step {i} delay");
        }
        // once current ≥ max/multiplier (10s/1.7 ≈ 5.882s) the cap value was
        // assigned directly and the interval stays at MaxInterval
        assert_eq!(b.current_interval(), Duration::from_secs(10));
        assert!(want4 >= Duration::from_nanos((10_000_000_000f64 / 1.7) as u64));
    }

    /// f=1 jitter bounds: random 0 → 0ns delay; random approaching 1 →
    /// ≈2×current. Both endpoints of the upstream interval formula.
    #[test]
    fn stream_preset_jitter_bounds_low_and_high() {
        let mut b = ExponentialBackoff::upstream_stream_default();
        let clock = MockClock::new();
        // random = 0 → min + 0×(max-min+1) = min = current - 1×current = 0
        let mut rng = FixedRng::new(&[0.0]);
        assert_eq!(b.next_delay(&clock, &mut rng), Some(Duration::ZERO));
        // next draw WITHOUT reset: current has grown to 1.36s, so
        // max = 2×1.36s = 2.72s (+1ns rounding of the upstream formula)
        let mut rng = FixedRng::new(&[0.999_999_999_999]);
        let got = b.next_delay(&clock, &mut rng).expect("not stopped");
        let max = Duration::from_nanos((2.0 * 1_360_000_000.0 + 1.0) as u64);
        assert!(
            got > Duration::from_secs(2) && got <= max,
            "high endpoint should be ≈2×current: got {got:?}, max {max:?}"
        );
    }

    /// Jitter stays inside `[current×(1−f), current×(1+f)]` for a sweep of
    /// samples (property, not exact values).
    #[test]
    fn jitter_stays_within_bounds_for_sweep() {
        let current = Duration::from_millis(800);
        let mut b = ExponentialBackoff::new(current, 1.0, 1.0, current, None); // multiplier 1 → constant interval
        let clock = MockClock::new();
        let mut rng = FixedRng::new(&[0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 0.999]);
        for _ in 0..7 {
            let d = b.next_delay(&clock, &mut rng).expect("not stopped");
            assert!(d <= 2 * current, "delay {d:?} above 2×current");
        }
    }

    /// Elapsed budget: past `max_elapsed`, `next_delay` returns `None`
    /// (Go `backoff.Stop`) and the caller must give up with the last error
    /// (upstream retry.go:34-39).
    #[test]
    fn max_elapsed_budget_stops_the_sequence() {
        let mut b = ExponentialBackoff::new(
            Duration::from_secs(1),
            0.0,
            1.0,
            Duration::from_secs(1),
            Some(Duration::from_secs(5)),
        );
        let mut clock = MockClock::new();
        // anchor the elapsed budget to the MOCK clock: `new()` starts the
        // budget on the monotonic clock, so without this reset the elapsed
        // time would mix two time bases
        b.reset(&clock);
        let mut rng = FixedRng::default();
        // t=0..4s: four 1s delays fit inside the 5s budget
        for i in 0..4 {
            assert!(b.next_delay(&clock, &mut rng).is_some(), "step {i} should not stop");
            clock.advance(Duration::from_secs(1));
        }
        // elapsed 4s + next 1s = 5s ≤ 5s → still allowed
        assert!(b.next_delay(&clock, &mut rng).is_some());
        clock.advance(Duration::from_secs(1)); // elapsed 5s
        assert_eq!(b.next_delay(&clock, &mut rng), None, "budget spent → Stop");
        // reset re-arms everything
        b.reset(&clock);
        assert!(b.next_delay(&clock, &mut rng).is_some());
    }

    /// Dial preset shape: 500ms base, f=0.5 → delay ∈ [250ms, 750ms] on the
    /// first draw; growth ×1.5; max_elapsed 10s (upstream dialer.go:22-27).
    #[test]
    fn dial_preset_matches_upstream_defaults() {
        let mut b = ExponentialBackoff::upstream_dial_default();
        let clock = MockClock::new();
        let mut rng = FixedRng::new(&[0.0, 1.0 - f64::EPSILON]);
        let low = b.next_delay(&clock, &mut rng).expect("low");
        assert_eq!(low, Duration::from_millis(250), "random=0 → min endpoint 0.5×500ms");
        // second draw from the grown interval 750ms: max = 1.5×750ms = 1125ms
        let high = b.next_delay(&clock, &mut rng).expect("high");
        assert!(
            high > Duration::from_millis(1100) && high <= Duration::from_millis(1126),
            "high endpoint ≈1.5×750ms, got {high:?}"
        );
    }

    /// Reset returns to the initial interval (upstream resets on every
    /// successful stream establishment — grpc.go:457).
    #[test]
    fn reset_restores_initial_interval() {
        let mut b = ExponentialBackoff::upstream_stream_default();
        let clock = MockClock::new();
        let mut rng = FixedRng::new(&[0.5]);
        let _ = b.next_delay(&clock, &mut rng);
        assert_ne!(b.current_interval(), Duration::from_millis(800));
        b.reset(&clock);
        assert_eq!(b.current_interval(), Duration::from_millis(800));
    }

    /// OsRandom produces values in [0, 1) (smoke — the distribution itself
    /// is the OS's).
    #[test]
    fn os_random_uniform_range() {
        let mut rng = OsRandom;
        for _ in 0..64 {
            let v = rng.next_uniform();
            assert!((0.0..1.0).contains(&v), "sample {v} outside [0,1)");
        }
    }
}
