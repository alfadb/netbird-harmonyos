//! CHUNK channel (gate-plan :419-431): structured detail transport for texts
//! that may exceed a single HiLog line or contain the `|` separator.
//!
//! `N1BDISC_CHUNK|stream=<s>|item=<id>|index=<i>|count=<c>|sha256=<h>|payload=<base64>`
//! - detail is UTF-8 bytes, sliced per 256 raw bytes
//! - payload = RFC 4648 base64 of the slice (alphabet has no `|`)
//! - sha256 covers the FULL pre-slice UTF-8 bytes
//! - frozen (stream, item) keys: dlerror/0, rejtext/<matrix id>, u3hex/0, foreign/0

use crate::hilog::emit;
use crate::util::{base64, sha256_hex};

pub const SLICE_MAX: usize = 256;

/// Stream key literals (frozen enumeration).
pub const STREAM_DLERROR: &str = "dlerror";
pub const STREAM_REJTEXT: &str = "rejtext";
pub const STREAM_U3HEX: &str = "u3hex";
pub const STREAM_FOREIGN: &str = "foreign";

pub fn emit_chunk(stream: &str, item: u32, detail: &str) {
    let bytes = detail.as_bytes();
    let sha = sha256_hex(bytes);
    let count = if bytes.is_empty() { 1 } else { (bytes.len() + SLICE_MAX - 1) / SLICE_MAX };
    for index in 0..count {
        let start = index * SLICE_MAX;
        let end = core::cmp::min(start + SLICE_MAX, bytes.len());
        let slice = if bytes.is_empty() { &bytes[..] } else { &bytes[start..end] };
        let payload = base64(slice);
        emit(&format!(
            "N1BDISC_CHUNK|stream={stream}|item={item}|index={index}|count={count}|sha256={sha}|payload={payload}"
        ));
    }
}
