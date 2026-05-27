/// Configuration loaded from %APPDATA%/mozkeys/config.toml.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level config structure.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub general:   GeneralConfig,
    pub movement:  MovementConfig,
    pub clicks:    ClickConfig,
    pub precision: PrecisionConfig,
    pub scroll:    ScrollConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// Trigger mode: "capslock_doubletap" | "capslock_hold" | "right_alt"
    pub mouse_mode: String,
    /// Maximum ms between taps to count as double-tap.
    pub double_tap_ms: u64,
    /// Key name to exit mouse mode (empty = same key as entry or Escape).
    pub exit_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct MovementConfig {
    pub up:    String,
    pub down:  String,
    pub left:  String,
    pub right: String,

    /// Pixels per tick at minimum speed.
    pub base_speed: f32,
    /// Pixels per tick at maximum speed.
    pub max_speed: f32,
    /// Acceleration factor (higher = faster ramp).
    pub acceleration: f32,
    /// Movement tick rate in Hz (recommended: 240).
    pub tick_rate: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ClickConfig {
    pub left:   String,
    pub right:  String,
    pub middle: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PrecisionConfig {
    /// Key that activates precision (slow) mode.
    pub modifier: String,
    /// Speed multiplier while precision is active (< 1.0).
    pub multiplier: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ScrollConfig {
    pub up:    String,
    pub down:  String,
    pub left:  String,
    pub right: String,
    /// Scroll lines per trigger tick.
    pub speed: i32,
}

// ── defaults ──────────────────────────────────────────────────────────────────

impl Default for Config {
    fn default() -> Self {
        Self {
            general:   GeneralConfig::default(),
            movement:  MovementConfig::default(),
            clicks:    ClickConfig::default(),
            precision: PrecisionConfig::default(),
            scroll:    ScrollConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            mouse_mode:    "capslock_doubletap".into(),
            double_tap_ms: 250,
            exit_key:      "escape".into(),
        }
    }
}

impl Default for MovementConfig {
    fn default() -> Self {
        Self {
            up:           "up".into(),
            down:         "down".into(),
            left:         "left".into(),
            right:        "right".into(),
            base_speed:   5.0,   // px/tick at 240 Hz ≈ 1200 px/s
            max_speed:    20.0,  // px/tick at 240 Hz ≈ 4800 px/s
            acceleration: 1.4,
            tick_rate:    240,
        }
    }
}

impl Default for ClickConfig {
    fn default() -> Self {
        Self {
            left:   "shift".into(),
            right:  "ctrl".into(),
            middle: "alt".into(),
        }
    }
}

impl Default for PrecisionConfig {
    fn default() -> Self {
        Self {
            modifier:   "capslock".into(),
            multiplier: 0.3,
        }
    }
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            up:    "pageup".into(),
            down:  "pagedown".into(),
            left:  "home".into(),
            right: "end".into(),
            speed: 3,
        }
    }
}

// ── I/O ───────────────────────────────────────────────────────────────────────

/// Returns the path to the config file: %APPDATA%\mozkeys\config.toml
pub fn config_path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("mozkeys").join("config.toml")
}

/// Load config from disk, falling back to defaults on any error.
/// Logs the error to stderr so the user knows what went wrong.
pub fn load() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(cfg) => {
                let validated = validate(cfg);
                eprintln!("[config] loaded from {}", path.display());
                validated
            }
            Err(e) => {
                eprintln!("[config] parse error in {}: {e} — using defaults", path.display());
                Config::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("[config] not found at {} — using defaults", path.display());
            // Write a default config so the user can edit it.
            let _ = write_default(&path);
            Config::default()
        }
        Err(e) => {
            eprintln!("[config] read error: {e} — using defaults");
            Config::default()
        }
    }
}

/// Clamp values to sane ranges to prevent undefined behaviour from bad configs.
fn validate(mut cfg: Config) -> Config {
    let m = &mut cfg.movement;
    m.base_speed   = m.base_speed.clamp(0.5, 100.0);
    m.max_speed    = m.max_speed.clamp(m.base_speed, 500.0);
    m.acceleration = m.acceleration.clamp(0.1, 50.0);
    m.tick_rate    = m.tick_rate.clamp(30, 1000);

    let p = &mut cfg.precision;
    p.multiplier   = p.multiplier.clamp(0.01, 1.0);

    let g = &mut cfg.general;
    g.double_tap_ms = g.double_tap_ms.clamp(50, 1000);

    cfg
}

/// Write a default config to disk so users have a template.
fn write_default(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let default_toml = r#"# mozkeys configuration
# Edit and save; the app reloads automatically on next startup.
# (hot reload can be added in a future release)

[general]
mouse_mode    = "capslock_doubletap"   # capslock_doubletap | capslock_hold | right_alt
double_tap_ms = 250                    # max ms between taps to register as double-tap
exit_key      = "escape"               # key to exit mouse mode

[movement]
up    = "up"
down  = "down"
left  = "left"
right = "right"

base_speed   = 5.0    # pixels/tick (5 px × 240 Hz ≈ 1200 px/s at start)
max_speed    = 20.0   # pixels/tick (20 px × 240 Hz ≈ 4800 px/s at max)
acceleration = 1.4    # ramp factor (higher = faster ramp)
tick_rate    = 240   # movement loop Hz

[clicks]
left   = "rctrl"
right  = "rshift"
middle = "/"

[precision]             
modifier   = "capslock"  # hold this key for slow precise movement
multiplier = 0.3         # speed multiplier in precision mode

[scroll]
up    = "pageup"
down  = "pagedown"
left  = "home"  
right = "end"
speed = 3   
"#;
    std::fs::write(path, default_toml)  ?;
    eprintln!("[config] wrote default config to {}", path.display());
    Ok(())  
}   
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      