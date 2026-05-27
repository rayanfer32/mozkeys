/// Mouse mode state machine.
///
/// Tracks whether we are in mouse mode and manages the double-tap detection
/// for CapsLock (or other configured trigger).
///
/// Thread-safety: all fields are atomic so both the hook thread (writer) and
/// movement loop (reader) can access them without a mutex.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::platform::windows::keycodes::{parse_key, Vk, VK_CAPITAL, VK_ESCAPE};
use crate::core::config::Config;

pub struct StateMachine {
    /// True while mouse mode is active.
    active: AtomicBool,
    /// Timestamp (µs) of the last trigger key-up event — used for double-tap.
    last_trigger_up_us: AtomicU64,
    /// Whether the trigger key is currently held (to handle hold mode).
    trigger_held: AtomicBool,

    // Resolved VK codes from config (set once at construction, never mutated).
    trigger_vk:   Vk,
    exit_vk:      Vk,
    mode:         TriggerMode,
    double_tap_us: u64,

    // Movement key VKs — needed to suppress them when in mouse mode.
    pub vk_up:    Vk,
    pub vk_down:  Vk,
    pub vk_left:  Vk,
    pub vk_right: Vk,

    // Click key VKs.
    pub vk_click_left:   Vk,
    pub vk_click_right:  Vk,
    pub vk_click_middle: Vk,

    // Precision modifier VK.
    pub vk_precision: Vk,

    // Scroll VKs.
    pub vk_scroll_up:    Vk,
    pub vk_scroll_down:  Vk,
    pub vk_scroll_left:  Vk,
    pub vk_scroll_right: Vk,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriggerMode {
    CapsLockDoubleTap,
    CapsLockHold,
    RightAlt,
}

impl StateMachine {
    pub fn new(cfg: &Config) -> Self {
        let mode = match cfg.general.mouse_mode.as_str() {
            "capslock_hold" => TriggerMode::CapsLockHold,
            "right_alt"     => TriggerMode::RightAlt,
            _               => TriggerMode::CapsLockDoubleTap,
        };

        let trigger_vk = match mode {
            TriggerMode::RightAlt => parse_key("ralt").unwrap(),
            _                     => VK_CAPITAL,
        };

        let exit_vk = parse_key(&cfg.general.exit_key)
            .unwrap_or(VK_ESCAPE);

        Self {
            active:             AtomicBool::new(false),
            last_trigger_up_us: AtomicU64::new(0),
            trigger_held:       AtomicBool::new(false),

            trigger_vk,
            exit_vk,
            mode,
            double_tap_us: cfg.general.double_tap_ms * 1_000,

            vk_up:    parse_key(&cfg.movement.up).unwrap_or(crate::platform::windows::keycodes::VK_UP),
            vk_down:  parse_key(&cfg.movement.down).unwrap_or(crate::platform::windows::keycodes::VK_DOWN),
            vk_left:  parse_key(&cfg.movement.left).unwrap_or(crate::platform::windows::keycodes::VK_LEFT),
            vk_right: parse_key(&cfg.movement.right).unwrap_or(crate::platform::windows::keycodes::VK_RIGHT),

            vk_click_left:   parse_key(&cfg.clicks.left).unwrap_or(crate::platform::windows::keycodes::VK_SHIFT),
            vk_click_right:  parse_key(&cfg.clicks.right).unwrap_or(crate::platform::windows::keycodes::VK_CONTROL),
            vk_click_middle: parse_key(&cfg.clicks.middle).unwrap_or(crate::platform::windows::keycodes::VK_MENU),

            vk_precision:    parse_key(&cfg.precision.modifier).unwrap_or(VK_CAPITAL),

            vk_scroll_up:    parse_key(&cfg.scroll.up).unwrap_or(crate::platform::windows::keycodes::VK_PRIOR),
            vk_scroll_down:  parse_key(&cfg.scroll.down).unwrap_or(crate::platform::windows::keycodes::VK_NEXT),
            vk_scroll_left:  parse_key(&cfg.scroll.left).unwrap_or(crate::platform::windows::keycodes::VK_HOME),
            vk_scroll_right: parse_key(&cfg.scroll.right).unwrap_or(crate::platform::windows::keycodes::VK_END),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// Called by hook on key-down.  Returns true if the key should be suppressed
    /// (not forwarded to the focused application).
    pub fn on_key_down(&self, vk: u16, _now_us: u64) -> bool {
        if vk == self.trigger_vk {
            self.trigger_held.store(true, Ordering::Release);
            if self.mode == TriggerMode::CapsLockHold {
                self.active.store(true, Ordering::Release);
            }
            // Suppress CapsLock to prevent toggling the caps indicator.
            return true;
        }

        let active = self.active.load(Ordering::Acquire);

        if vk == self.exit_vk && active {
            self.active.store(false, Ordering::Release);
            return true;
        }

        if active {
            // Suppress all mouse-mode keys so they don't type characters.
            return self.is_mouse_mode_key(vk);
        }

        false
    }

    /// Called by hook on key-up.  Returns true if key should be suppressed.
    pub fn on_key_up(&self, vk: u16, now_us: u64) -> bool {
        if vk == self.trigger_vk {
            self.trigger_held.store(false, Ordering::Release);

            match self.mode {
                TriggerMode::CapsLockDoubleTap => {
                    let last = self.last_trigger_up_us.load(Ordering::Acquire);
                    if last != 0 && now_us.saturating_sub(last) <= self.double_tap_us {
                        // Toggle mouse mode.
                        let was_active = self.active.fetch_xor(true, Ordering::AcqRel);
                        self.last_trigger_up_us.store(0, Ordering::Release);
                        eprintln!("[sm] mouse mode {}", if !was_active { "ON" } else { "OFF" });
                    } else {
                        self.last_trigger_up_us.store(now_us, Ordering::Release);
                    }
                }
                TriggerMode::CapsLockHold => {
                    self.active.store(false, Ordering::Release);
                }
                TriggerMode::RightAlt => {
                    // Toggle on release.
                    let was_active = self.active.fetch_xor(true, Ordering::AcqRel);
                    eprintln!("[sm] mouse mode {}", if !was_active { "ON" } else { "OFF" });
                }
            }
            return true; // always suppress trigger key
        }

        let active = self.active.load(Ordering::Acquire);
        if active {
            return self.is_mouse_mode_key(vk);
        }

        false
    }

    /// True if `vk` is a key that should be suppressed while in mouse mode.
    ///
    /// Uses `vk_matches` so that:
    ///   - a generic configured key (e.g. "shift" = VK_SHIFT 0x10) suppresses
    ///     both VK_LSHIFT and VK_RSHIFT.
    ///   - a specific configured key (e.g. "rshift" = VK_RSHIFT 0xA1) suppresses
    ///     only VK_RSHIFT; VK_LSHIFT passes through normally.
    fn is_mouse_mode_key(&self, vk: u16) -> bool {
        Self::vk_matches(vk, self.vk_click_left)
            || Self::vk_matches(vk, self.vk_click_right)
            || Self::vk_matches(vk, self.vk_click_middle)
            || Self::vk_matches(vk, self.vk_precision)
            || vk == self.vk_up
            || vk == self.vk_down
            || vk == self.vk_left
            || vk == self.vk_right
            || vk == self.vk_scroll_up
            || vk == self.vk_scroll_down
            || vk == self.vk_scroll_left
            || vk == self.vk_scroll_right
            || vk == self.exit_vk
    }

    /// Returns true if incoming `vk` corresponds to configured `target`.
    ///
    /// When `target` is a *generic* modifier (VK_SHIFT/CONTROL/MENU), expands
    /// to both left and right variants — e.g. "shift" matches LSHIFT and RSHIFT.
    ///
    /// When `target` is already *specific* (VK_LSHIFT, VK_RSHIFT, etc.), requires
    /// an exact match — e.g. "rshift" only matches VK_RSHIFT, not VK_LSHIFT.
    #[inline]
    fn vk_matches(vk: u16, target: u16) -> bool {
        if vk == target { return true; }
        match target {
            0x10 => matches!(vk, 0xA0 | 0xA1), // VK_SHIFT    → LSHIFT or RSHIFT
            0x11 => matches!(vk, 0xA2 | 0xA3), // VK_CONTROL  → LCTRL  or RCTRL
            0x12 => matches!(vk, 0xA4 | 0xA5), // VK_MENU     → LALT   or RALT
            _    => false,
        }
    }
}
