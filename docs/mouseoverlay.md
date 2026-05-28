NeatMouse overlay Implementation Details:
1. Overlay Window Creation
A special layered window (WS_EX_LAYERED) is created with WS_POPUP | WS_VISIBLE styles in the ThreadProc() function (lines 153-165)
The window is 16x16 pixels and positioned relative to the cursor with an offset equal to half the cursor size
WS_EX_NOACTIVATE prevents the window from stealing focus from other applications
2. Mouse Hook for Position Tracking
A low-level mouse hook (WH_MOUSE_LL) is installed in MouseProc() (lines 42-52)
On every WM_MOUSEMOVE event, the function calls PostRedrawOverlay() to update the overlay position
The hook avoids using MSLLHOOKSTRUCT to work correctly on multi-monitor setups with different DPI settings
3. Drawing the Icon
The DrawOverlay() function (lines 56-77) renders the bitmap using:

Layered Window Blending: UpdateLayeredWindow() is called with AC_SRC_OVER blend operation
Alpha Blending: The bitmap's alpha channel (AC_SRC_ALPHA) is used for transparency
16x16 Icon: Loads a PNG image (IDB_PNG_NEATMOUSE) and renders it at the overlay window
4. Position Update
The RedrawOverlay() function (lines 28-37) continuously:

Gets the current cursor position via GetCursorPos()
Offsets it by half the cursor dimensions (dx, dy)
Uses SetWindowPos() with HWND_TOPMOST to keep the overlay always on top and aligned with the cursor
5. Threading Model
The overlay runs in a separate thread (ThreadProc()) with its own message loop
This prevents blocking the main application thread
The thread is created via _beginthreadex() when overlay is enabled
This design creates a responsive visual indicator that follows the cursor in real-time without impacting the main application's performance.