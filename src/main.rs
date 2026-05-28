#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// mozkeys — keyboard-driven mouse control for Windows.
///
/// Entry point:    
///   1. Initialise QPC timer.
///   2. Load conf  ig.
///   3. Build shared state (KeyStateTable,  StateMachine).
///   4. Spawn movement loop thread.
///   5. Spawn hook thread (installs WH_KEYBOARD_LL and pumps messages).
///   6. Spawn tray thread (polls for tray exit).
///   7. Join threads (runs until tray exit).

mod core;
mod platform;
mod runtime;    

use std::sync::Arc;

fn main() {
    // ── 1. Init high-resolution timer ────────────────────────────────────────
    platform::windows::timers::init();

    // ── 2. Load config ───────────────────────────────────────────────────────
    let cfg = core::config::load();
    eprintln!("[main] tick_rate={}Hz base_speed={} max_speed={}",
        cfg.movement.tick_rate, cfg.movement.base_speed, cfg.movement.max_speed);

    // ── 3. Build shared state ────────────────────────────────────────────────
    let key_states    = Arc::new(core::key_state::KeyStateTable::new());
    let state_machine = Arc::new(core::state_machine::StateMachine::new(&cfg));

    // ── 4. Spawn movement loop ────────────────────────────────────────────────
    let _movement_handle = runtime::movement_loop::spawn_movement_loop(
        Arc::clone(&key_states),
        Arc::clone(&state_machine),
        &cfg,
    );

    // ── 5. Spawn hook thread (blocks internally on GetMessageW loop) ──────────
    let hook_handle = platform::windows::hook::spawn_hook_thread(
        Arc::clone(&key_states),
        Arc::clone(&state_machine),
    );

    // ── 6. Spawn emoji overlay (colored dot follows cursor in mouse mode) ─────
    let _overlay_handle = platform::windows::overlay::spawn_overlay_thread(
        Arc::clone(&state_machine),
    );

    // ── 7. Spawn tray thread (shows icon and handles exit) ────────────────────
    let _tray_handle = platform::windows::tray::spawn_tray_thread(
        Arc::clone(&state_machine),
    );

    eprintln!("[main] running — double-tap CapsLock to enter mouse mode");

    // ── 8. Block until hook thread exits (i.e., forever) ─────────────────────
    let _ = hook_handle.join();
}
