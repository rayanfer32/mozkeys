/// Low-level keyboard hook for Windows.
///
/// Installs a WH_KEYBOARD_LL hook via SetWindowsHookExW and pumps the
/// associated message loop on a dedicated thread.  The hook callback runs
/// synchronously on that same thread — no cross-thread dispatch needed for
/// key event delivery.
///
/// Architecture:
///   hook thread
///     ├── SetWindowsHookExW(WH_KEYBOARD_LL, hook_proc)
///     ├── GetMessageW loop  (pumps hook callbacks)
///     └── on each key event → writes into shared KeyStateTable via atomics
///
/// The hook proc MUST return quickly (< a few ms) or Windows will time-out
/// the hook and bypass it.  We only do atomic writes — no allocations, no
/// mutexes, no I/O.

use std::sync::Arc;
use std::thread;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

use crate::core::key_state::KeyStateTable;
use crate::core::state_machine::StateMachine;
use crate::platform::windows::timers;

/// Spawns the hook thread.  Returns a join-handle; the thread runs until the
/// process exits (no clean shutdown mechanism — acceptable for a tray utility).
pub fn spawn_hook_thread(
    key_states: Arc<KeyStateTable>,
    state_machine: Arc<StateMachine>,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("hook".into())
        .spawn(move || run_hook_loop(key_states, state_machine))
        .expect("failed to spawn hook thread")
}

// ── thread-local hook context ─────────────────────────────────────────────────
// The WH_KEYBOARD_LL callback is a plain C function pointer; we store context
// in thread-locals so the callback can access it without unsafe global statics.

thread_local! {
    static TL_KEY_STATES: std::cell::Cell<*const KeyStateTable> =
        std::cell::Cell::new(std::ptr::null());
    static TL_STATE_MACHINE: std::cell::Cell<*const StateMachine> =
        std::cell::Cell::new(std::ptr::null());
}

fn run_hook_loop(key_states: Arc<KeyStateTable>, state_machine: Arc<StateMachine>) {
    // Store raw pointers in thread-local for the callback.
    // Safety: the Arcs are alive for the entire thread lifetime.
    TL_KEY_STATES.with(|cell| cell.set(Arc::as_ptr(&key_states)));
    TL_STATE_MACHINE.with(|cell| cell.set(Arc::as_ptr(&state_machine)));

    let hook = unsafe {
        let hmod = GetModuleHandleW(None).unwrap_or_default();
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod, 0)
            .expect("SetWindowsHookExW failed")
    };

    // Standard Win32 message loop — required to service the hook.
    // GetMessageW blocks until a message arrives (no busy-wait).
    let mut msg = MSG::default();
    loop {
        unsafe {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 <= 0 {
                break; // WM_QUIT or error
            }
        }
    }

    unsafe { let _ = UnhookWindowsHookEx(hook); }
}

/// The actual hook callback — runs on the hook thread.
/// Must be as fast as possible; no allocations, no blocking.
unsafe extern "system" fn hook_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: lparam points to a KBDLLHOOKSTRUCT when code == HC_ACTION.
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode as u16;
        let now_us = timers::now_us();

        let is_down = matches!(
            wparam.0 as u32,
            WM_KEYDOWN | WM_SYSKEYDOWN
        );
        let is_up = matches!(
            wparam.0 as u32,
            WM_KEYUP | WM_SYSKEYUP
        );

        // LLKHF_INJECTED (bit 4): event was injected — skip to avoid feedback loops.
        // We do NOT filter injected events from ourselves because we don't inject
        // keyboard events, only mouse events.  But third-party injectors could cause
        // issues; we skip them to be safe.
        const LLKHF_INJECTED: u32 = 0x10;
        if kb.flags.0 & LLKHF_INJECTED != 0 {
            return CallNextHookEx(HHOOK::default(), code, wparam, lparam);
        }

        let key_states_ptr = TL_KEY_STATES.with(|c| c.get());
        let sm_ptr = TL_STATE_MACHINE.with(|c| c.get());

        if !key_states_ptr.is_null() && !sm_ptr.is_null() {
            let key_states = &*key_states_ptr;
            let sm = &*sm_ptr;

            if is_down {
                key_states.set_down(vk, now_us);
                let suppress = sm.on_key_down(vk, now_us);
                if suppress {
                    // Return non-zero to suppress key from reaching other apps.
                    return LRESULT(1);
                }
            } else if is_up {
                key_states.set_up(vk, now_us);
                let suppress = sm.on_key_up(vk, now_us);
                if suppress {
                    return LRESULT(1);
                }
            }
        }
    }

    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}
