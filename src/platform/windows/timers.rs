#![allow(dead_code)]
/// High-resolution timer utilities for Windows.
///
/// We use QueryPerformanceCounter (QPC) for all timing. QPC is:
/// - monotonic
/// - sub-microsecond resolution
/// - safe to call from any thread
/// - immune to NTP/clock adjustments

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

static mut QPC_FREQ: i64 = 0;

/// Must be called once at startup (single-threaded context).
pub fn init() {
    unsafe {
        let mut freq = 0i64;
        QueryPerformanceFrequency(&mut freq).expect("QPC not available");
        QPC_FREQ = freq;
    }
}

/// Returns the current time in microseconds since an arbitrary epoch.
/// Thread-safe after `init()`.
#[inline(always)]
pub fn now_us() -> u64 {
    unsafe {
        let mut counter = 0i64;
        QueryPerformanceCounter(&mut counter).expect("QPC failed");
        // Multiply first to avoid precision loss: (counter * 1_000_000) / freq
        (counter as u64).wrapping_mul(1_000_000) / QPC_FREQ as u64
    }
}

/// Returns current time in milliseconds.
#[inline(always)]
pub fn now_ms() -> u64 {
    now_us() / 1_000
}
