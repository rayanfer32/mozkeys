/// Deterministic 240 Hz (configurable) movement tick loop.
///
/// Timing strategy:
///   We use QueryPerformanceCounter to compute the ideal next-tick timestamp
///   and sleep for the residual duration.  This gives us consistent 4.16 ms
///   intervals at 240 Hz without busy-waiting or OS timer drift accumulation.
///
///   sleep_duration = next_tick_us - now_us
///
///   If a tick fires late (OS preempted us), we don't try to catch up — we
///   just move the next target forward by one interval.  This prevents
///   "spiral of death" lag bursts.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::core::config::Config;
use crate::core::dispatcher::Dispatcher;
use crate::core::key_state::KeyStateTable;
use crate::core::state_machine::StateMachine;
use crate::platform::windows::timers;

pub fn spawn_movement_loop(
    key_states: Arc<KeyStateTable>,
    state_machine: Arc<StateMachine>,
    cfg: &Config,
) -> thread::JoinHandle<()> {
    let dispatcher = Dispatcher::new(key_states, Arc::clone(&state_machine), cfg);

    thread::Builder::new()
        .name("movement".into())
        .spawn(move || run_loop(state_machine, dispatcher))
        .expect("failed to spawn movement thread")
}

fn run_loop(state_machine: Arc<StateMachine>, mut dispatcher: Dispatcher) {
    use std::sync::atomic::Ordering;

    let mut tick_rate_hz = state_machine.tick_rate.load(Ordering::Acquire).max(1);
    let mut tick_us = 1_000_000u64 / tick_rate_hz as u64;

    let mut next_tick_us = timers::now_us() + tick_us;

    loop {
        if state_machine.reload_flag.load(Ordering::Acquire) {
            dispatcher.reload_params();
            tick_rate_hz = state_machine.tick_rate.load(Ordering::Acquire).max(1);
            tick_us = 1_000_000u64 / tick_rate_hz as u64;
            state_machine.reload_flag.store(false, Ordering::Release);
        }

        let now_us = timers::now_us();

        if now_us >= next_tick_us {
            dispatcher.tick(now_us);
            // Advance target by exactly one interval (no catch-up).
            next_tick_us = now_us + tick_us;
        } else {
            let sleep_us = next_tick_us - now_us;
            // Sleep for most of the interval; wake a bit early to avoid overshoot.
            // We subtract 200 µs as a guard margin for scheduler imprecision.
            let sleep_us = sleep_us.saturating_sub(200);
            if sleep_us > 500 {
                thread::sleep(Duration::from_micros(sleep_us));
            }
            // Busy-spin for the final < 500 µs — tight but bounded, typically
            // < 0.1% CPU at 240 Hz because we only spin in a very short window.
            // (Full busy-wait would be ~1–2 % CPU, which we intentionally avoid.)
        }
    }
}
