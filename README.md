# mozkeys

A lightweight, low-latency keyboard-driven mouse control utility for Windows.
Control your cursor entirely from the keyboard — no mouse required.

## Features

- **Double-tap CapsLock** to enter/exit mouse mode
- Arrow keys move the cursor with smooth acceleration
- Left, right, and middle click simulation
- Vertical and horizontal scrolling
- Precision (slow) mode for fine positioning
- 240 Hz deterministic movement loop — no OS key-repeat jitter
- < 5 ms input processing latency
- ~612 KB binary, negligible CPU/RAM usage
- Fully configurable via a TOML file

## Usage

Run the executable. It starts silently in the background.

```
mozkeys.exe
```

**Default bindings:**

| Key | Action |
|---|---|
| CapsLock × 2 (≤ 250 ms) | Enter / exit mouse mode |
| Arrow keys | Move cursor |
| Hold CapsLock | Precision (slow) mode |
| Shift | Left click |
| Ctrl | Right click |
| Alt | Middle click |
| Page Up / Page Down | Scroll up / down |
| Home / End | Scroll left / right |
| Escape | Exit mouse mode |

All bindings and speed values are configurable.

## Configuration

On first run, a default config is written to:

```
%APPDATA%\mozkeys\config.toml
```

Example:

```toml
[general]
mouse_mode    = "capslock_doubletap"   # capslock_doubletap | capslock_hold | right_alt
double_tap_ms = 250
exit_key      = "escape"

[movement]
up    = "up"
down  = "down"
left  = "left"
right = "right"

base_speed   = 4.0    # pixels/tick at start of hold
max_speed    = 28.0   # pixels/tick after full acceleration
acceleration = 1.4    # ramp factor
tick_rate    = 240    # movement loop Hz

[clicks]
left   = "shift"
right  = "ctrl"
middle = "alt"

[precision]
modifier   = "capslock"
multiplier = 0.3      # speed multiplier in precision mode

[scroll]
up    = "pageup"
down  = "pagedown"
left  = "home"
right = "end"
speed = 3
```

Restart the application to apply config changes.

## Building

Requires Rust (stable) and a Windows target.

```sh
cargo build --release
```

Binary: `target/release/mozkeys.exe`

## Architecture

```
src/
├── main.rs                    entry point, thread wiring
├── platform/windows/
│   ├── hook.rs                WH_KEYBOARD_LL hook + Win32 message loop
│   ├── send_input.rs          SendInput wrappers (move, click, scroll)
│   ├── keycodes.rs            VK constants + config key name parser
│   └── timers.rs              QueryPerformanceCounter (sub-microsecond timer)
├── core/
│   ├── key_state.rs           lock-free atomic key state table
│   ├── state_machine.rs       mouse mode toggle logic
│   ├── acceleration.rs        velocity curve: v = base + accel × t^1.5
│   ├── dispatcher.rs          per-tick movement / click / scroll dispatch
│   └── config.rs              TOML config load, validate, write defaults
└── runtime/
    └── movement_loop.rs       deterministic 240 Hz QPC tick loop
```

**Threading model:**
- **Hook thread** — installs `WH_KEYBOARD_LL`, runs `GetMessageW`. Callback does only atomic writes; no allocations, no locks, returns in < 1 µs.
- **Movement thread** — 240 Hz loop driven by `QueryPerformanceCounter`. Reads key states atomically, computes cursor delta, dispatches `SendInput`.

## Acceleration

Cursor speed follows a smooth power curve:

```
v(t) = clamp(base_speed + acceleration × t^1.5, base_speed, max_speed)
```

where `t` is seconds the movement key has been held. This gives instant response at `base_speed`, a gentle ramp, then a plateau at `max_speed`. Precision mode multiplies velocity by `precision.multiplier` (default 0.3).

## Mouse mode triggers

| Config value | Behaviour |
|---|---|
| `capslock_doubletap` | Two taps of CapsLock within `double_tap_ms` toggles mode |
| `capslock_hold` | Mouse mode active while CapsLock is held |
| `right_alt` | Right Alt toggles mouse mode |

## Requirements

- Windows 10 or later (x86-64)
- No installation required — single executable
- No elevated privileges required for normal use
