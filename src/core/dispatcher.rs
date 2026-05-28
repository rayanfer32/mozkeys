use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::core::acceleration::AccelParams;
use crate::core::config::Config;
use crate::core::key_state::KeyStateTable;
use crate::core::state_machine::StateMachine;
use crate::platform::windows::send_input::{self, MouseButton};

pub struct Dispatcher {
    key_states:    Arc<KeyStateTable>,
    state_machine: Arc<StateMachine>,
    accel:         AccelParams,
    scroll_speed:  i32,

    // Sub-pixel accumulator for smooth movement at low speeds.
    accum_x: f32,
    accum_y: f32,

    // Click de-bounce: track which click keys were already fired this press.
    click_left_fired:   bool,
    click_right_fired:  bool,
    click_middle_fired: bool,
    scroll_up_fired:   bool,
    scroll_down_fired: bool,
    scroll_left_fired: bool,
    scroll_right_fired: bool,
}

impl Dispatcher {
    pub fn new(
        key_states: Arc<KeyStateTable>,
        state_machine: Arc<StateMachine>,
        cfg: &Config,
    ) -> Self {
        let mut d = Self {
            key_states,
            state_machine,
            accel: AccelParams {
                base_speed:    cfg.movement.base_speed,
                max_speed:     cfg.movement.max_speed,
                acceleration:  cfg.movement.acceleration,
                precision_mul: cfg.precision.multiplier,
            },
            scroll_speed: cfg.scroll.speed,
            accum_x: 0.0,
            accum_y: 0.0,
            click_left_fired:   false,
            click_right_fired:  false,
            click_middle_fired: false,
            scroll_up_fired:   false,
            scroll_down_fired: false,
            scroll_left_fired: false,
            scroll_right_fired: false,
        };
        d.reload_params();
        d
    }

    pub fn reload_params(&mut self) {
        let sm = &*self.state_machine;
        self.accel.base_speed = f32::from_bits(sm.base_speed.load(Ordering::Acquire));
        self.accel.max_speed = f32::from_bits(sm.max_speed.load(Ordering::Acquire));
        self.accel.acceleration = f32::from_bits(sm.acceleration.load(Ordering::Acquire));
        self.accel.precision_mul = f32::from_bits(sm.precision_multiplier.load(Ordering::Acquire));
        self.scroll_speed = sm.scroll_speed.load(Ordering::Acquire);
    }

    /// Called once per movement tick from the movement loop.
    pub fn tick(&mut self, now_us: u64) {
        if !self.state_machine.is_active() {
            // Not in mouse mode: reset accumulators so there's no burst on entry.
            self.accum_x = 0.0;
            self.accum_y = 0.0;
            self.reset_click_state();
            return;
        }

        // ── Read all state into locals — releases borrows of self ─────────────
        let (
            precision,
            up_held, down_held, left_held, right_held,
            held_up, held_down, held_left, held_right,
            click_l, click_r, click_m,
            scroll_u, scroll_d, scroll_l, scroll_r,
        ) = {
            let sm = &*self.state_machine;
            let ks = &*self.key_states;
            (
                ks.is_down(sm.vk_precision.load(Ordering::Acquire)),
                ks.is_down(sm.vk_up.load(Ordering::Acquire)),
                ks.is_down(sm.vk_down.load(Ordering::Acquire)),
                ks.is_down(sm.vk_left.load(Ordering::Acquire)),
                ks.is_down(sm.vk_right.load(Ordering::Acquire)),
                ks.held_duration_us(sm.vk_up.load(Ordering::Acquire),    now_us),
                ks.held_duration_us(sm.vk_down.load(Ordering::Acquire),  now_us),
                ks.held_duration_us(sm.vk_left.load(Ordering::Acquire), now_us),
                ks.held_duration_us(sm.vk_right.load(Ordering::Acquire), now_us),
                ks.is_down(sm.vk_click_left.load(Ordering::Acquire)),
                ks.is_down(sm.vk_click_right.load(Ordering::Acquire)),
                ks.is_down(sm.vk_click_middle.load(Ordering::Acquire)),
                ks.is_down(sm.vk_scroll_up.load(Ordering::Acquire)),
                ks.is_down(sm.vk_scroll_down.load(Ordering::Acquire)),
                ks.is_down(sm.vk_scroll_left.load(Ordering::Acquire)),
                ks.is_down(sm.vk_scroll_right.load(Ordering::Acquire)),
            )
        };
        // sm and ks are no longer borrowed here.

        // ── cursor movement ───────────────────────────────────────────────────
        let moving = up_held || down_held || left_held || right_held;

        if moving {
            let max_held = [held_up, held_down, held_left, held_right]
                .into_iter().max().unwrap_or(0);

            let speed = self.accel.speed(max_held, precision);

            let dx_dir = (right_held as i32 - left_held as i32) as f32;
            let dy_dir = (down_held  as i32 - up_held   as i32) as f32;

            // Normalise diagonals so speed is consistent in all 8 directions.
            let (dx_dir, dy_dir) = if dx_dir != 0.0 && dy_dir != 0.0 {
                let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
                (dx_dir * inv_sqrt2, dy_dir * inv_sqrt2)
            } else {
                (dx_dir, dy_dir)
            };

            self.accum_x += dx_dir * speed;
            self.accum_y += dy_dir * speed;

            let px = self.accum_x as i32;
            let py = self.accum_y as i32;
            if px != 0 || py != 0 {
                send_input::move_cursor(px, py);
                self.accum_x -= px as f32;
                self.accum_y -= py as f32;
            }
        } else {
            self.accum_x = 0.0;
            self.accum_y = 0.0;
        }

        // ── clicks (hold to drag, release to let go) ──────────────────────────
        if click_l && !self.click_left_fired {
            send_input::press(MouseButton::Left);
            self.click_left_fired = true;
        } else if !click_l && self.click_left_fired {
            send_input::release(MouseButton::Left);
            self.click_left_fired = false;
        }

        if click_r && !self.click_right_fired {
            send_input::press(MouseButton::Right);
            self.click_right_fired = true;
        } else if !click_r && self.click_right_fired {
            send_input::release(MouseButton::Right);
            self.click_right_fired = false;
        }

        if click_m && !self.click_middle_fired {
            send_input::press(MouseButton::Middle);
            self.click_middle_fired = true;
        } else if !click_m && self.click_middle_fired {
            send_input::release(MouseButton::Middle);
            self.click_middle_fired = false;
        }

        // ── scrolling ─────────────────────────────────────────────────────────
        if scroll_u && !self.scroll_up_fired {
            send_input::scroll_vertical(self.scroll_speed);
            self.scroll_up_fired = true;
        } else if !scroll_u {
            self.scroll_up_fired = false;
        }

        if scroll_d && !self.scroll_down_fired {
            send_input::scroll_vertical(-self.scroll_speed);
            self.scroll_down_fired = true;
        } else if !scroll_d {
            self.scroll_down_fired = false;
        }

        if scroll_l && !self.scroll_left_fired {
            send_input::scroll_horizontal(-self.scroll_speed);
            self.scroll_left_fired = true;
        } else if !scroll_l {
            self.scroll_left_fired = false;
        }

        if scroll_r && !self.scroll_right_fired {
            send_input::scroll_horizontal(self.scroll_speed);
            self.scroll_right_fired = true;
        } else if !scroll_r {
            self.scroll_right_fired = false;
        }
    }

    fn reset_click_state(&mut self) {
        if self.click_left_fired {
            send_input::release(MouseButton::Left);
            self.click_left_fired = false;
        }
        if self.click_right_fired {
            send_input::release(MouseButton::Right);
            self.click_right_fired = false;
        }
        if self.click_middle_fired {
            send_input::release(MouseButton::Middle);
            self.click_middle_fired = false;
        }
        self.scroll_up_fired   = false;
        self.scroll_down_fired = false;
        self.scroll_left_fired = false;
        self.scroll_right_fired = false;
    }
}
