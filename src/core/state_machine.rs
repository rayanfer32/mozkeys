/// Mouse mode state machine.
///
/// Tracks whether we are in mouse mode and manages the double-tap detection
/// for CapsLock (or other configured trigger).
///
/// Thread-safety: all fields are atomic so both the hook thread (writer) and
/// movement loop (reader) can access them without a mutex.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU16, AtomicU8, AtomicU32, AtomicI32, Ordering};

use crate::platform::windows::keycodes::{parse_key, VK_CAPITAL, VK_ESCAPE};
use crate::core::config::Config;
use crate::core::key_state::KeyStateTable;

pub struct StateMachine {
    /// True while mouse mode is active.
    active: AtomicBool,
    /// Timestamp (µs) of the last trigger key-up event — used for double-tap.
    last_trigger_up_us: AtomicU64,
    /// Whether the trigger key is currently held (to handle hold mode).
    trigger_held: AtomicBool,

    // Resolved VK codes from config (dynamic)
    pub trigger_vk:   AtomicU16,
    pub exit_vk:      AtomicU16,
    mode_val:         AtomicU8, // 0 = CapsLockDoubleTap, 1 = CapsLockHold, 2 = RightAlt, 3 = RCtrlRShift
    pub double_tap_us: AtomicU64,

    // Movement key VKs — needed to suppress them when in mouse mode.
    pub vk_up:    AtomicU16,
    pub vk_down:  AtomicU16,
    pub vk_left:  AtomicU16,
    pub vk_right: AtomicU16,

    // Click key VKs.
    pub vk_click_left:   AtomicU16,
    pub vk_click_right:  AtomicU16,
    pub vk_click_middle: AtomicU16,

    // Precision modifier VK.
    pub vk_precision: AtomicU16,

    // Scroll VKs.
    pub vk_scroll_up:    AtomicU16,
    pub vk_scroll_down:  AtomicU16,
    pub vk_scroll_left:  AtomicU16,
    pub vk_scroll_right: AtomicU16,

    // Dispatcher configuration values (floats stored as u32 bits)
    pub base_speed:           AtomicU32,
    pub max_speed:            AtomicU32,
    pub acceleration:         AtomicU32,
    pub precision_multiplier: AtomicU32,
    pub scroll_speed:         AtomicI32,
    pub tick_rate:            AtomicU32,

    // Feature toggles and flags
    pub overlay_enabled: AtomicBool,
    pub reload_flag:     AtomicBool,

    // Chord trigger state
    pub rctrl_rshift_triggered: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriggerMode {
    CapsLockDoubleTap,
    CapsLockHold,
    RightAlt,
    RCtrlRShift,
}

impl StateMachine {
    pub fn new(cfg: &Config) -> Self {
        let sm = Self {
            active:             AtomicBool::new(true),
            last_trigger_up_us: AtomicU64::new(0),
            trigger_held:       AtomicBool::new(false),

            trigger_vk:   AtomicU16::new(0),
            exit_vk:      AtomicU16::new(0),
            mode_val:         AtomicU8::new(0),
            double_tap_us: AtomicU64::new(0),

            vk_up:    AtomicU16::new(0),
            vk_down:  AtomicU16::new(0),
            vk_left:  AtomicU16::new(0),
            vk_right: AtomicU16::new(0),

            vk_click_left:   AtomicU16::new(0),
            vk_click_right:  AtomicU16::new(0),
            vk_click_middle: AtomicU16::new(0),

            vk_precision: AtomicU16::new(0),

            vk_scroll_up:    AtomicU16::new(0),
            vk_scroll_down:  AtomicU16::new(0),
            vk_scroll_left:  AtomicU16::new(0),
            vk_scroll_right: AtomicU16::new(0),

            base_speed:           AtomicU32::new(0),
            max_speed:            AtomicU32::new(0),
            acceleration:         AtomicU32::new(0),
            precision_multiplier: AtomicU32::new(0),
            scroll_speed:         AtomicI32::new(0),
            tick_rate:            AtomicU32::new(0),

            overlay_enabled: AtomicBool::new(true),
            reload_flag:     AtomicBool::new(false),

            rctrl_rshift_triggered: AtomicBool::new(false),
        };

        sm.reload_config(cfg);
        sm.reload_flag.store(false, Ordering::Release); // clear reload flag on creation
        sm
    }

    pub fn reload_config(&self, cfg: &Config) {
        let mode = match cfg.general.mouse_mode.as_str() {
            "capslock_hold" => TriggerMode::CapsLockHold,
            "right_alt"     => TriggerMode::RightAlt,
            "rctrl_rshift"  => TriggerMode::RCtrlRShift,
            _               => TriggerMode::CapsLockDoubleTap,
        };

        let mode_val = match mode {
            TriggerMode::CapsLockDoubleTap => 0,
            TriggerMode::CapsLockHold => 1,
            TriggerMode::RightAlt => 2,
            TriggerMode::RCtrlRShift => 3,
        };
        self.mode_val.store(mode_val, Ordering::Release);

        let trigger_vk = match mode {
            TriggerMode::RightAlt => parse_key("ralt").unwrap(),
            _                     => VK_CAPITAL,
        };
        self.trigger_vk.store(trigger_vk, Ordering::Release);

        let exit_vk = parse_key(&cfg.general.exit_key).unwrap_or(VK_ESCAPE);
        self.exit_vk.store(exit_vk, Ordering::Release);

        self.double_tap_us.store(cfg.general.double_tap_ms * 1_000, Ordering::Release);

        self.vk_up.store(parse_key(&cfg.movement.up).unwrap_or(crate::platform::windows::keycodes::VK_UP), Ordering::Release);
        self.vk_down.store(parse_key(&cfg.movement.down).unwrap_or(crate::platform::windows::keycodes::VK_DOWN), Ordering::Release);
        self.vk_left.store(parse_key(&cfg.movement.left).unwrap_or(crate::platform::windows::keycodes::VK_LEFT), Ordering::Release);
        self.vk_right.store(parse_key(&cfg.movement.right).unwrap_or(crate::platform::windows::keycodes::VK_RIGHT), Ordering::Release);

        self.vk_click_left.store(parse_key(&cfg.clicks.left).unwrap_or(crate::platform::windows::keycodes::VK_SHIFT), Ordering::Release);
        self.vk_click_right.store(parse_key(&cfg.clicks.right).unwrap_or(crate::platform::windows::keycodes::VK_CONTROL), Ordering::Release);
        self.vk_click_middle.store(parse_key(&cfg.clicks.middle).unwrap_or(crate::platform::windows::keycodes::VK_MENU), Ordering::Release);

        self.vk_precision.store(parse_key(&cfg.precision.modifier).unwrap_or(VK_CAPITAL), Ordering::Release);

        self.vk_scroll_up.store(parse_key(&cfg.scroll.up).unwrap_or(crate::platform::windows::keycodes::VK_PRIOR), Ordering::Release);
        self.vk_scroll_down.store(parse_key(&cfg.scroll.down).unwrap_or(crate::platform::windows::keycodes::VK_NEXT), Ordering::Release);
        self.vk_scroll_left.store(parse_key(&cfg.scroll.left).unwrap_or(crate::platform::windows::keycodes::VK_HOME), Ordering::Release);
        self.vk_scroll_right.store(parse_key(&cfg.scroll.right).unwrap_or(crate::platform::windows::keycodes::VK_END), Ordering::Release);

        self.base_speed.store(cfg.movement.base_speed.to_bits(), Ordering::Release);
        self.max_speed.store(cfg.movement.max_speed.to_bits(), Ordering::Release);
        self.acceleration.store(cfg.movement.acceleration.to_bits(), Ordering::Release);
        self.precision_multiplier.store(cfg.precision.multiplier.to_bits(), Ordering::Release);
        self.scroll_speed.store(cfg.scroll.speed, Ordering::Release);
        self.tick_rate.store(cfg.movement.tick_rate, Ordering::Release);

        self.reload_flag.store(true, Ordering::Release);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    #[allow(dead_code)]
    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Release);
        eprintln!("[sm] mouse mode set to {}", if active { "ON" } else { "OFF" });
    }

    pub fn toggle_active(&self) -> bool {
        let prev = self.active.fetch_xor(true, Ordering::AcqRel);
        let new_val = !prev;
        eprintln!("[sm] mouse mode toggled to {}", if new_val { "ON" } else { "OFF" });
        new_val
    }

    fn get_mode(&self) -> TriggerMode {
        match self.mode_val.load(Ordering::Acquire) {
            1 => TriggerMode::CapsLockHold,
            2 => TriggerMode::RightAlt,
            3 => TriggerMode::RCtrlRShift,
            _ => TriggerMode::CapsLockDoubleTap,
        }
    }

    /// Called by hook on key-down.  Returns true if the key should be suppressed
    /// (not forwarded to the focused application).
    pub fn on_key_down(&self, vk: u16, _now_us: u64, key_states: &KeyStateTable) -> bool {
        let mode = self.get_mode();
        if mode == TriggerMode::RCtrlRShift {
            if vk == 0xA3 || vk == 0xA1 {
                let other_key = if vk == 0xA3 { 0xA1 } else { 0xA3 };
                if key_states.is_down(other_key) {
                    if !self.rctrl_rshift_triggered.load(Ordering::Acquire) {
                        self.rctrl_rshift_triggered.store(true, Ordering::Release);
                        self.toggle_active();
                    }
                }
                return true; // Always suppress trigger keys
            }
        } else {
            let trigger_vk = self.trigger_vk.load(Ordering::Acquire);
            if vk == trigger_vk {
                self.trigger_held.store(true, Ordering::Release);
                if mode == TriggerMode::CapsLockHold {
                    self.active.store(true, Ordering::Release);
                }
                // Suppress CapsLock to prevent toggling the caps indicator.
                return true;
            }
        }

        let active = self.active.load(Ordering::Acquire);

        let exit_vk = self.exit_vk.load(Ordering::Acquire);
        if vk == exit_vk && active {
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
    pub fn on_key_up(&self, vk: u16, now_us: u64, _key_states: &KeyStateTable) -> bool {
        let mode = self.get_mode();
        if mode == TriggerMode::RCtrlRShift {
            if vk == 0xA3 || vk == 0xA1 {
                self.rctrl_rshift_triggered.store(false, Ordering::Release);
                return true; // Always suppress trigger keys
            }
        } else {
            let trigger_vk = self.trigger_vk.load(Ordering::Acquire);
            if vk == trigger_vk {
                self.trigger_held.store(false, Ordering::Release);

                match self.get_mode() {
                    TriggerMode::CapsLockDoubleTap => {
                        let last = self.last_trigger_up_us.load(Ordering::Acquire);
                        let double_tap_us = self.double_tap_us.load(Ordering::Acquire);
                        if last != 0 && now_us.saturating_sub(last) <= double_tap_us {
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
                    _ => {}
                }
                return true; // always suppress trigger key
            }
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
        let is_trigger = if self.get_mode() == TriggerMode::RCtrlRShift {
            vk == 0xA3 || vk == 0xA1
        } else {
            vk == self.trigger_vk.load(Ordering::Acquire)
        };
        is_trigger
            || Self::vk_matches(vk, self.vk_click_left.load(Ordering::Acquire))
            || Self::vk_matches(vk, self.vk_click_right.load(Ordering::Acquire))
            || Self::vk_matches(vk, self.vk_click_middle.load(Ordering::Acquire))
            || Self::vk_matches(vk, self.vk_precision.load(Ordering::Acquire))
            || vk == self.vk_up.load(Ordering::Acquire)
            || vk == self.vk_down.load(Ordering::Acquire)
            || vk == self.vk_left.load(Ordering::Acquire)
            || vk == self.vk_right.load(Ordering::Acquire)
            || vk == self.vk_scroll_up.load(Ordering::Acquire)
            || vk == self.vk_scroll_down.load(Ordering::Acquire)
            || vk == self.vk_scroll_left.load(Ordering::Acquire)
            || vk == self.vk_scroll_right.load(Ordering::Acquire)
            || vk == self.exit_vk.load(Ordering::Acquire)
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


