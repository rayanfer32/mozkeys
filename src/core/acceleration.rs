/// Acceleration model for cursor movement.
///
/// Velocity is computed as:
///
///   v(t) = clamp(base + accel * t^exponent, base, max)
///
/// where t = seconds held.
///
/// This gives:
///   - instant response at base_speed (no dead zone)
///   - smooth polynomial ramp (not linear)
///   - hard cap at max_speed
///
/// `precision_mul` is applied multiplicatively when precision mode is active.

#[derive(Clone)]
pub struct AccelParams {
    pub base_speed:    f32,
    pub max_speed:     f32,
    pub acceleration:  f32,  // pixels per second² equivalent
    pub precision_mul: f32,
}

impl AccelParams {
    /// Compute cursor speed (pixels per tick) given:
    ///   - `held_us`:      microseconds the key has been held
    ///   - `tick_rate_hz`: movement loop frequency
    ///   - `precision`:    whether precision mode is active
    #[inline]
    pub fn speed(&self, held_us: u64, tick_rate_hz: u32, precision: bool) -> f32 {
        let held_s = held_us as f32 * 1e-6;
        // Smooth power curve: v = base + accel * held_s^1.5
        // The 1.5 exponent gives a gentle initial ramp, then faster growth.
        let v = self.base_speed + self.acceleration * held_s.powf(1.5);
        let v = v.min(self.max_speed);
        let v = if precision { v * self.precision_mul } else { v };
        // Convert speed (px/s) to pixels per tick.
        v / tick_rate_hz as f32
    }
}
