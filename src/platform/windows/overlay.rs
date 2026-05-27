/// Cursor overlay — colored indicator follows the cursor while mouse mode is on.
///
/// Implementation follows the NeatMouse strategy exactly:
///   1. A WS_EX_LAYERED topmost popup window (click-through via WS_EX_TRANSPARENT).
///   2. A 32-bit BGRA DIB section with pre-multiplied alpha generated at startup.
///   3. UpdateLayeredWindow + ULW_ALPHA for per-pixel transparency — no colorkey,
///      no GDI text rendering; every pixel is fully colored.
///
/// A dedicated thread owns the window and runs its own GetMessageW loop.
/// A 60 Hz WM_TIMER polls StateMachine::is_active() and the cursor position,
/// calling UpdateLayeredWindow to reposition the overlay or ShowWindow to hide it.

use std::sync::Arc;
use std::thread;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, DIB_RGB_COLORS,
    GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, RGBQUAD, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetCursorPos, GetMessageW,
    PostQuitMessage, RegisterClassExW, SetTimer, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, MSG, SW_HIDE,
    SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSEXW, WM_DESTROY, WM_TIMER,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP, HWND_TOPMOST, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
};

use crate::core::state_machine::StateMachine;

/// Icon dimensions in pixels.
const ICON_SIZE: i32 = 20;

/// Pixel offset from cursor hotspot to the top-left corner of the overlay.
/// Positioned just to the bottom-right of the tip so it doesn't obscure it.
const OFFSET_X: i32 = 14;
const OFFSET_Y: i32 = 18;

const TIMER_ID: usize = 1;
const TIMER_MS: u32   = 16; // ~60 Hz

// ── Per-thread GDI context ────────────────────────────────────────────────────

/// GDI resources kept alive for the overlay thread's lifetime.
struct OverlayCtx {
    hdc_mem:  HDC,
    hbitmap:  HBITMAP,
    old_obj:  HGDIOBJ,
    visible:  bool,
}

thread_local! {
    static TL_SM:  std::cell::Cell<*const StateMachine> =
        std::cell::Cell::new(std::ptr::null());
    static TL_CTX: std::cell::Cell<*mut OverlayCtx> =
        std::cell::Cell::new(std::ptr::null_mut());
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn spawn_overlay_thread(state_machine: Arc<StateMachine>) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("overlay".into())
        .spawn(move || run_overlay(state_machine))
        .expect("failed to spawn overlay thread")
}

// ── Thread body ───────────────────────────────────────────────────────────────

fn run_overlay(state_machine: Arc<StateMachine>) {
    // Store raw pointer in TLS so the WNDPROC callback (a C fn pointer) can reach it.
    // Safety: the Arc keeps StateMachine alive for the entire thread lifetime.
    TL_SM.with(|c| c.set(Arc::as_ptr(&state_machine)));

    unsafe {
        let hmod       = GetModuleHandleW(None).unwrap_or_default();
        let class_name = windows::core::w!("mozkeys_overlay");

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
            // LAYERED  → required for UpdateLayeredWindow
            // TRANSPARENT → clicks pass through to windows beneath
            // TOPMOST  → always on top
            // NOACTIVATE → never steals keyboard focus
            // TOOLWINDOW → excluded from Alt-Tab
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST
                | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            class_name,
            windows::core::PCWSTR::null(),
            WS_POPUP,
            CW_USEDEFAULT, CW_USEDEFAULT, ICON_SIZE, ICON_SIZE,
            None, None, hmod, None,
        )
        .expect("CreateWindowExW (overlay) failed");

        // ── Build the icon DIB ────────────────────────────────────────────────
        // generate_icon() returns pre-multiplied BGRA bytes (required by ULW_ALPHA).
        let pixels = generate_icon(ICON_SIZE as usize);

        let bmi = make_bmi(ICON_SIZE);
        let mut pvbits: *mut std::ffi::c_void = std::ptr::null_mut();

        // CreateDIBSection with null HDC creates a screen-compatible 32-bit bitmap.
        let hbitmap = match CreateDIBSection(
            HDC(std::ptr::null_mut()),
            &bmi,
            DIB_RGB_COLORS,
            &mut pvbits,
            None,
            0,
        ) {
            Ok(h) if !h.is_invalid() => h,
            _ => {
                eprintln!("[overlay] CreateDIBSection failed — overlay disabled");
                return;
            }
        };

        // Copy our pre-multiplied BGRA pixels into the DIB's pixel buffer.
        std::ptr::copy_nonoverlapping(
            pixels.as_ptr(),
            pvbits as *mut u8,
            pixels.len(),
        );

        // Create a memory DC and select the bitmap so we can pass it to UpdateLayeredWindow.
        // CreateCompatibleDC(null) creates a memory DC compatible with the screen.
        let hdc_mem = CreateCompatibleDC(HDC(std::ptr::null_mut()));
        let old_obj = SelectObject(hdc_mem, HGDIOBJ(hbitmap.0 as *mut _));

        let ctx = Box::new(OverlayCtx { hdc_mem, hbitmap, old_obj, visible: false });
        TL_CTX.with(|c| c.set(Box::into_raw(ctx)));

        // 60 Hz update timer.
        let _ = SetTimer(hwnd, TIMER_ID, TIMER_MS, None);

        // Standard Win32 message loop — blocks here until WM_QUIT.
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 <= 0 { break; }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }
}

// ── Window procedure ──────────────────────────────────────────────────────────

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER => {
            let sm_ptr  = TL_SM.with(|c| c.get());
            let ctx_ptr = TL_CTX.with(|c| c.get());
            if sm_ptr.is_null() || ctx_ptr.is_null() {
                return LRESULT(0);
            }
            let sm  = &*sm_ptr;
            let ctx = &mut *ctx_ptr;

            if sm.is_active() {
                // ── Reposition the overlay to follow the cursor ───────────────
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);

                let pt_dst = POINT { x: pt.x + OFFSET_X, y: pt.y + OFFSET_Y };
                let pt_src = POINT { x: 0, y: 0 };
                let sz     = SIZE  { cx: ICON_SIZE, cy: ICON_SIZE };

                // ULW_ALPHA: use the bitmap's per-pixel alpha channel.
                // SourceConstantAlpha=255: no additional constant transparency.
                // AC_SRC_ALPHA: bitmap has pre-multiplied alpha.
                let blend = BLENDFUNCTION {
                    BlendOp:             AC_SRC_OVER as u8,
                    BlendFlags:          0,
                    SourceConstantAlpha: 255,
                    AlphaFormat:         AC_SRC_ALPHA as u8,
                };

                // hdcDst must be the screen DC so Windows can composite correctly.
                let hdc_screen = GetDC(None);
                let _ = UpdateLayeredWindow(
                    hwnd,
                    hdc_screen,
                    Some(&pt_dst),   // new window position on screen
                    Some(&sz),
                    ctx.hdc_mem,     // memory DC with our BGRA bitmap selected
                    Some(&pt_src),   // top-left of source bitmap
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                ReleaseDC(None, hdc_screen);

                // Show the window the first time it becomes active.
                if !ctx.visible {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    ctx.visible = true;
                }

                // Force the window to stay topmost, even when switching focus/windows.
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            } else if ctx.visible {
                // Mouse mode turned off — hide the overlay.
                let _ = ShowWindow(hwnd, SW_HIDE);
                ctx.visible = false;
            }

            LRESULT(0)
        }

        WM_DESTROY => {
            // Release GDI resources.
            let ctx_ptr = TL_CTX.with(|c| c.get());
            if !ctx_ptr.is_null() {
                let ctx = Box::from_raw(ctx_ptr);
                SelectObject(ctx.hdc_mem, ctx.old_obj);
                let _ = DeleteDC(ctx.hdc_mem);
                let _ = DeleteObject(HGDIOBJ(ctx.hbitmap.0 as *mut _));
                TL_CTX.with(|c| c.set(std::ptr::null_mut()));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ── Icon generation ───────────────────────────────────────────────────────────

/// Build a top-down 32-bit BGRA BITMAPINFO.
fn make_bmi(size: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize:          std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth:         size,
            biHeight:        -size,  // negative = top-down (row 0 is the top)
            biPlanes:        1,
            biBitCount:      32,     // BGRA
            biCompression:   BI_RGB.0, // no compression; alpha lives in the high byte
            biSizeImage:     0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed:       0,
            biClrImportant:  0,
        },
        bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
    }
}

/// Generate a colored lime-green circle indicator as **pre-multiplied BGRA** bytes.
///
/// UpdateLayeredWindow with ULW_ALPHA requires pre-multiplied alpha:
///   stored_B = pixel_B * alpha / 255   (same for G and R)
///   stored_A = alpha
///
/// The icon has:
///   - a bright lime-green fill (inner ~60% of radius)
///   - a darker forest-green border ring
///   - smooth anti-aliased edges via coverage sampling
fn generate_icon(n: usize) -> Vec<u8> {
    let mut px = vec![0u8; n * n * 4]; // BGRA, pre-multiplied

    let cx = n as f32 * 0.5;
    let cy = n as f32 * 0.5;
    let r_fill   = cx * 0.60; // inner lime fill radius
    let r_border = cx - 1.0;  // outer dark-ring radius
    let aa       = 1.0_f32;   // anti-alias half-width in pixels

    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d  = (dx * dx + dy * dy).sqrt();

            // Colour palette (straight RGB):
            //   lime fill  : R=80  G=200 B=40   (#50C828  ≈ lime green)
            //   dark border: R=20  G=100 B=10   (#14640A  ≈ forest green)
            let (r, g, b, a): (u32, u32, u32, u32) = if d < r_fill - aa {
                (80, 200, 40, 255)
            } else if d < r_fill + aa {
                // Smooth transition from fill to border.
                let t  = ((d - (r_fill - aa)) / (aa * 2.0)).clamp(0.0, 1.0);
                let lp = |lo: u32, hi: u32| (lo as f32 * (1.0 - t) + hi as f32 * t) as u32;
                (lp(80, 20), lp(200, 100), lp(40, 10), 255)
            } else if d < r_border - aa {
                (20, 100, 10, 255)
            } else if d < r_border + aa {
                // Smooth anti-aliased outer edge → transparent.
                let t     = ((d - (r_border - aa)) / (aa * 2.0)).clamp(0.0, 1.0);
                let alpha = ((1.0 - t) * 255.0) as u32;
                (20, 100, 10, alpha)
            } else {
                (0, 0, 0, 0) // fully transparent outside the circle
            };

            let i = (y * n + x) * 4;
            // Pre-multiply: stored = channel * alpha / 255.
            // Windows compositor requires this for correct blending with ULW_ALPHA.
            px[i + 0] = (b * a / 255) as u8; // B
            px[i + 1] = (g * a / 255) as u8; // G
            px[i + 2] = (r * a / 255) as u8; // R
            px[i + 3] = a as u8;             // A
        }
    }
    px
}