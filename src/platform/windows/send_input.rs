/// Thin wrapper around SendInput for mouse actions.
///
/// All cursor movement uses MOUSEEVENTF_MOVE (relative).
/// We never use SetCursorPos so movement respects DPI and acceleration settings
/// only as far as the raw pixel delta — acceleration is computed in our engine,
/// not the OS mouse acceleration curve (which would double-accelerate).

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_HWHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS,
};
/// Move the cursor by (dx, dy) pixels relative to current position.
/// dx > 0 → right, dy > 0 → down.
#[inline]
pub fn move_cursor(dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }
    let input = build_mouse_input(MOUSEEVENTF_MOVE, dx, dy, 0);
    send(&[input]);
}

/// Simulate a mouse button click (down + up in one SendInput call for atomicity).
pub fn click(button: MouseButton) {
    let (down_flag, up_flag) = match button {
        MouseButton::Left   => (MOUSEEVENTF_LEFTDOWN,   MOUSEEVENTF_LEFTUP),
        MouseButton::Right  => (MOUSEEVENTF_RIGHTDOWN,  MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    let inputs = [
        build_mouse_input(down_flag, 0, 0, 0),
        build_mouse_input(up_flag,   0, 0, 0),
    ];
    send(&inputs);
}

/// Scroll vertically. `delta` > 0 scrolls up, < 0 scrolls down.
/// Windows uses WHEEL_DELTA=120 as one notch.
pub fn scroll_vertical(delta: i32) {
    let input = build_mouse_input(MOUSEEVENTF_WHEEL, 0, 0, delta * 120);
    send(&[input]);
}

/// Scroll horizontally. `delta` > 0 scrolls right, < 0 scrolls left.
pub fn scroll_horizontal(delta: i32) {
    let input = build_mouse_input(MOUSEEVENTF_HWHEEL, 0, 0, delta * 120);
    send(&[input]);
}

#[derive(Clone, Copy, Debug)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn build_mouse_input(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: i32) -> INPUT {
    let mut input = INPUT::default();
    input.r#type = INPUT_MOUSE;
    // SAFETY: the union field `mi` is the correct variant for INPUT_MOUSE.
    input.Anonymous.mi = MOUSEINPUT {
            dx,
            dy,
            mouseData: data as u32,
            dwFlags: flags,
            time: 0,        // let the system timestamp
            dwExtraInfo: 0,
        };
    input
}

#[inline]
fn send(inputs: &[INPUT]) {
    // Return value is number of events inserted; 0 means blocked (UIPI).
    // We ignore the return value intentionally — there's nothing useful we
    // can do if the OS blocks injection (e.g., UAC prompts).
    unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
}
