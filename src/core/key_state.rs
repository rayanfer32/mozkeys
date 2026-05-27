#![allow(dead_code)]
/// Per-key state table.
///
/// Tracks whether each Virtual Key is currently physically held, plus the
/// timestamp of the last transition.  Uses a flat array of atomics indexed
/// by VK code (0..=255) for O(1) access with no locking.
///
/// Two u64 atomics per key:
///   - `down[vk]`  : timestamp_us of last key-down, or 0 if currently up.
///   - `up[vk]`    : timestamp_us of last key-up, or 0 if never released.

use std::sync::atomic::{AtomicU64, Ordering};

const VK_COUNT: usize = 256;

pub struct KeyStateTable {
    /// Timestamp (µs) of last keydown.  0 = currently released.
    down: [AtomicU64; VK_COUNT],
    /// Timestamp (µs) of last keyup.
    up: [AtomicU64; VK_COUNT],
}

impl KeyStateTable {
    pub fn new() -> Self {
        // AtomicU64 doesn't implement Copy; initialise with a macro trick.
        Self {
            down: std::array::from_fn(|_| AtomicU64::new(0)),
            up:   std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Called from the hook callback on key-down.
    #[inline(always)]
    pub fn set_down(&self, vk: u16, timestamp_us: u64) {
        let idx = vk as usize & 0xFF;
        self.down[idx].store(timestamp_us, Ordering::Release);
    }

    /// Called from the hook callback on key-up.
    #[inline(always)]
    pub fn set_up(&self, vk: u16, timestamp_us: u64) {
        let idx = vk as usize & 0xFF;
        self.down[idx].store(0, Ordering::Release);
        self.up[idx].store(timestamp_us, Ordering::Release);
    }

    /// Returns true if the key is currently held down.
    #[inline(always)]
    pub fn is_down(&self, vk: u16) -> bool {
        self.down[vk as usize & 0xFF].load(Ordering::Acquire) != 0
    }

    /// Returns the timestamp (µs) at which the key was pressed, or 0 if up.
    #[inline(always)]
    pub fn down_since(&self, vk: u16) -> u64 {
        self.down[vk as usize & 0xFF].load(Ordering::Acquire)
    }

    /// How long (µs) the key has been held, given `now_us`.
    /// Returns 0 if the key is not currently down.
    #[inline(always)]
    pub fn held_duration_us(&self, vk: u16, now_us: u64) -> u64 {
        let since = self.down_since(vk);
        if since == 0 { 0 } else { now_us.saturating_sub(since) }
    }

    /// Force-release all keys.  Used after sleep/wake or focus loss to prevent
    /// stuck keys.
    pub fn reset_all(&self) {
        for i in 0..VK_COUNT {
            self.down[i].store(0, Ordering::Release);
        }
    }
}
