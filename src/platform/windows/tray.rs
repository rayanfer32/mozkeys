use std::thread;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, POINT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NOTIFYICONDATAW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, LoadIconW, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, TrackPopupMenu, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, MSG, TPM_LEFTALIGN, TPM_RIGHTBUTTON,
    WNDCLASSEXW, WS_POPUP, WM_COMMAND, WM_DESTROY, WM_USER, IDI_APPLICATION,
    MF_GRAYED, MF_SEPARATOR, MF_STRING,
};

const WM_TRAY_CALLBACK: u32 = WM_USER + 100;
const WM_RBUTTONUP: u32 = 0x0205; // standard Win32 message for right click up

const MENU_TITLE_ID: u32 = 1001;
const MENU_EXIT_ID: u32 = 1002;

pub fn spawn_tray_thread() -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("tray".into())
        .spawn(move || run_tray())
        .expect("failed to spawn tray thread")
}

fn run_tray() {
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

unsafe fn show_popup_menu(hwnd: HWND) {
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let _ = SetForegroundWindow(hwnd);

    let hmenu = CreatePopupMenu().expect("failed to create menu");
    
    // Grayed/disabled header
    let _ = AppendMenuW(
        hmenu,
        MF_GRAYED,
        MENU_TITLE_ID as usize,
        windows::core::w!("mozkeys (Running)")
    );
    // Separator line
    let _ = AppendMenuW(
        hmenu,
        MF_SEPARATOR,
        0,
        windows::core::PCWSTR::null()
    );
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
            if event == WM_RBUTTONUP {
                show_popup_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 as u32;
            if id == MENU_EXIT_ID {
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
