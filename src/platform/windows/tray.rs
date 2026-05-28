use std::thread;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, POINT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, TrackPopupMenu, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, MSG, TPM_LEFTALIGN, TPM_RIGHTBUTTON,
    WNDCLASSEXW, WS_POPUP, WM_COMMAND, WM_DESTROY, WM_USER, IDI_APPLICATION,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, SW_SHOW,
};

use crate::core::state_machine::StateMachine;

const WM_TRAY_CALLBACK: u32 = WM_USER + 100;
const WM_RBUTTONUP: u32 = 0x0205; // standard Win32 message for right click up
const WM_LBUTTONDBLCLK: u32 = 0x0203; // standard Win32 message for left click double click

const MENU_TITLE_ID: u32    = 1001;
const MENU_TOGGLE_ID: u32   = 1002;
const MENU_EDIT_ID: u32     = 1003;
const MENU_RELOAD_ID: u32   = 1004;
const MENU_OVERLAY_ID: u32  = 1005;
const MENU_STARTUP_ID: u32  = 1006;
const MENU_EXIT_ID: u32     = 1007;
const MENU_RESET_ID: u32    = 1008;

thread_local! {
    static TL_SM: std::cell::Cell<*const StateMachine> =
        std::cell::Cell::new(std::ptr::null());
}

pub fn spawn_tray_thread(state_machine: Arc<StateMachine>) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("tray".into())
        .spawn(move || run_tray(state_machine))
        .expect("failed to spawn tray thread")
}

fn run_tray(state_machine: Arc<StateMachine>) {
    // Store in TLS so the static wnd_proc can access the state machine.
    TL_SM.with(|c| c.set(Arc::as_ptr(&state_machine)));

    unsafe {
        let hmod = GetModuleHandleW(None).unwrap_or_default();
        let class_name = windows::core::w!("mozkeys_tray");

        let wc = WNDCLASSEXW {
            cbSize:         std::mem::size_of::<WNDCLASSEXW>() as u32,
            style:          CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc:    Some(wnd_proc),
            hInstance:      hmod.into(),
            lpszClassName:  class_name,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);

        let hwnd = CreateWindowExW(
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
            class_name,
            windows::core::PCWSTR::null(),
            WS_POPUP,
            0, 0, 0, 0,
            None, None, hmod, None,
        )
        .expect("CreateWindowExW (tray) failed");

        // Load the icon from executable resources (ID 1)
        let hicon = match LoadIconW(hmod, windows::core::PCWSTR(1 as *const u16)) {
            Ok(h) if !h.is_invalid() => h,
            _ => LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        };

        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_CALLBACK;
        nid.hIcon = hicon;

        let tip = windows::core::w!("mozkeys");
        let tip_slice = tip.as_wide();
        for (i, &ch) in tip_slice.iter().enumerate() {
            if i < nid.szTip.len() {
                nid.szTip[i] = ch;
            }
        }

        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        // Standard Win32 message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

unsafe fn delete_tray_icon(hwnd: HWND) {
    let mut nid = NOTIFYICONDATAW::default();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
}

// Check if app is set to run at startup in Windows Registry
fn is_startup_enabled() -> bool {
    if let Ok(exe_path) = std::env::current_exe() {
        let output = std::process::Command::new("reg")
            .args(&["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "mozkeys"])
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                return stdout.contains(exe_path.to_str().unwrap_or(""));
            }
        }
    }
    false
}

// Toggle Windows Registry startup key
fn toggle_startup() {
    if is_startup_enabled() {
        let _ = std::process::Command::new("reg")
            .args(&["delete", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "mozkeys", "/f"])
            .output();
    } else {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_str) = exe_path.to_str() {
                let value_data = format!("\"{}\"", exe_str);
                let _ = std::process::Command::new("reg")
                    .args(&["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "mozkeys", "/t", "REG_SZ", "/d", &value_data, "/f"])
                    .output();
            }
        }
    }
}

// Open config in default editor
unsafe fn edit_config() {
    let path = crate::core::config::config_path();
    if let Some(path_str) = path.to_str() {
        let path_w: Vec<u16> = path_str.encode_utf16().chain(Some(0)).collect();
        let _ = ShellExecuteW(
            None,
            windows::core::w!("open"),
            windows::core::PCWSTR(path_w.as_ptr()),
            None,
            None,
            SW_SHOW,
        );
    }
}

unsafe fn show_popup_menu(hwnd: HWND, state_machine: &StateMachine) {
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let _ = SetForegroundWindow(hwnd);

    let hmenu = CreatePopupMenu().expect("failed to create menu");
    
    // Dynamic Title showing active status
    let is_active = state_machine.is_active();
    let title_w = if is_active {
        windows::core::w!("mozkeys (Mouse Mode ON)")
    } else {
        windows::core::w!("mozkeys (Mouse Mode OFF)")
    };
    let _ = AppendMenuW(
        hmenu,
        MF_GRAYED,
        MENU_TITLE_ID as usize,
        title_w
    );
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());

    // Toggle active state
    let toggle_text = if is_active {
        windows::core::w!("Disable Mouse Mode")
    } else {
        windows::core::w!("Enable Mouse Mode")
    };
    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        MENU_TOGGLE_ID as usize,
        toggle_text
    );
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());

    // Edit Settings
    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        MENU_EDIT_ID as usize,
        windows::core::w!("Edit Settings (config.toml)")
    );

    // Reload Settings
    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        MENU_RELOAD_ID as usize,
        windows::core::w!("Reload Configuration")
    );

    // Reset Settings
    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        MENU_RESET_ID as usize,
        windows::core::w!("Reset to Default Settings")
    );
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());

    // Toggle Cursor Indicator checkbox
    let overlay_flags = if state_machine.overlay_enabled.load(Ordering::Acquire) {
        MF_STRING | windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS(0x00000008) // MF_CHECKED
    } else {
        MF_STRING | windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS(0x00000000) // MF_UNCHECKED
    };
    let _ = AppendMenuW(
        hmenu,
        overlay_flags,
        MENU_OVERLAY_ID as usize,
        windows::core::w!("Show Cursor Indicator")
    );

    // Run at Startup checkbox
    let startup_flags = if is_startup_enabled() {
        MF_STRING | windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS(0x00000008) // MF_CHECKED
    } else {
        MF_STRING | windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS(0x00000000) // MF_UNCHECKED
    };
    let _ = AppendMenuW(
        hmenu,
        startup_flags,
        MENU_STARTUP_ID as usize,
        windows::core::w!("Start with Windows")
    );
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, windows::core::PCWSTR::null());

    // Exit button
    let _ = AppendMenuW(
        hmenu,
        MF_STRING,
        MENU_EXIT_ID as usize,
        windows::core::w!("Exit")
    );

    let _ = TrackPopupMenu(
        hmenu,
        TPM_LEFTALIGN | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        hwnd,
        None,
    );

    let _ = DestroyMenu(hmenu);
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY_CALLBACK => {
            let event = lparam.0 as u32;
            let sm_ptr = TL_SM.with(|c| c.get());
            if !sm_ptr.is_null() {
                if event == WM_RBUTTONUP {
                    show_popup_menu(hwnd, &*sm_ptr);
                } else if event == WM_LBUTTONDBLCLK {
                    (*sm_ptr).toggle_active();
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 as u32;
            let sm_ptr = TL_SM.with(|c| c.get());
            if !sm_ptr.is_null() {
                let sm = &*sm_ptr;
                match id {
                    MENU_TOGGLE_ID => {
                        sm.toggle_active();
                    }
                    MENU_EDIT_ID => {
                        edit_config();
                    }
                    MENU_RELOAD_ID => {
                        let cfg = crate::core::config::load();
                        sm.reload_config(&cfg);
                    }
                    MENU_RESET_ID => {
                        if let Ok(_) = crate::core::config::reset_to_default() {
                            let cfg = crate::core::config::load();
                            sm.reload_config(&cfg);
                            eprintln!("[tray] configuration reset to defaults");
                        }
                    }
                    MENU_OVERLAY_ID => {
                        let prev = sm.overlay_enabled.fetch_xor(true, Ordering::AcqRel);
                        eprintln!("[tray] cursor indicator {}", if !prev { "ON" } else { "OFF" });
                    }
                    MENU_STARTUP_ID => {
                        toggle_startup();
                    }
                    MENU_EXIT_ID => {
                        delete_tray_icon(hwnd);
                        std::process::exit(0);
                    }
                    _ => {}
                }
            } else if id == MENU_EXIT_ID {
                delete_tray_icon(hwnd);
                std::process::exit(0);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            delete_tray_icon(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

