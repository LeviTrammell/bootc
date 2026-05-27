mod actions;
mod button;
mod gamepad_merger;
mod input;
mod joystick_cursor;
mod keyboard_grid;
mod niri;
mod state;
mod wtype;
mod ydotool;

use actions::{ActionResult, Modifiers};
use button::{Button, ButtonState};
use gamepad_merger::GamepadMerger;
use joystick_cursor::JoystickCursor;
use keyboard_grid::KeyboardGrid;
use state::NavMode;

use anyhow::Result;
use log::{debug, info, warn};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::time;

/// How long L1+R1+Start must be held to toggle passthrough.
const PASSTHROUGH_HOLD_MS: u64 = 500;

/// How long L1+R1+Select must be held to switch VT.
const VT_SWITCH_HOLD_MS: u64 = 500;

/// Initial delay before key repeat starts (ms).
const REPEAT_DELAY_MS: u64 = 400;

/// Interval between repeated key actions (ms).
const REPEAT_INTERVAL_MS: u64 = 150;

/// Buttons that support software key repeat when held.
/// In TextEntry mode, shoulder buttons (L1/R1) also repeat for cursor movement.
fn is_repeatable(button: Button, mode: NavMode) -> bool {
    match mode {
        NavMode::TextEntry => matches!(
            button,
            Button::DpadUp
                | Button::DpadDown
                | Button::DpadLeft
                | Button::DpadRight
                | Button::L1
                | Button::R1
        ),
        _ => matches!(
            button,
            Button::DpadUp | Button::DpadDown | Button::DpadLeft | Button::DpadRight
        ),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("niri-nav starting");

    // Ensure WAYLAND_DISPLAY is set (may be missing when started from
    // systemd user service or SSH). Scan XDG_RUNTIME_DIR for the socket.
    if std::env::var("WAYLAND_DISPLAY").is_err() {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let rd = PathBuf::from(&runtime_dir);
            if let Ok(entries) = std::fs::read_dir(&rd) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if name.starts_with("wayland-") && !name.ends_with(".lock") {
                        info!("Auto-detected WAYLAND_DISPLAY={}", name);
                        std::env::set_var("WAYLAND_DISPLAY", &*name);
                        break;
                    }
                }
            }
        }
    }

    // Spawn joystick-cursor daemon (analog stick → virtual pointer)
    let mut cursor = JoystickCursor::new();
    if let Err(e) = cursor.spawn() {
        warn!("Failed to spawn joystick-cursor: {} (cursor control disabled)", e);
    }

    // Spawn gamepad-merger daemon (merges 3 evdev devices → 1 virtual gamepad)
    let mut merger = GamepadMerger::new();
    if let Err(e) = merger.spawn() {
        warn!("Failed to spawn gamepad-merger: {} (gamepad passthrough disabled)", e);
    }

    // Open gamepad device and convert to async event stream
    let device = input::open_gamepad()?;
    let mut stream = device
        .into_event_stream()
        .map_err(|e| anyhow::anyhow!("Failed to create event stream: {}", e))?;

    let mut mode = NavMode::WindowNav;
    let mut mods = Modifiers::default();
    let mut grid = KeyboardGrid::new();

    // Grab the device in initial mode
    input::grab(stream.device_mut())?;
    info!("Mode: {}", mode);

    // Passthrough combo tracking
    let mut passthrough_combo_start: Option<Instant> = None;

    // VT switch combo tracking (L1+R1+Select -> sudo chvt 2)
    let mut vt_switch_combo_start: Option<Instant> = None;
    let mut vt_switched_away = false; // true when we ungrabbed for TTY2

    // Key repeat tracking
    let mut held_button: Option<Button> = None;
    let mut repeat_deadline: Option<Instant> = None;

    // Track which buttons are currently held
    let mut held_buttons: HashSet<Button> = HashSet::new();

    // Stub for future text-input-v3 integration (detect when a text field is focused)
    let text_input_active: bool = false;
    let _ = &text_input_active; // suppress unused warning until wired up

    loop {
        // Calculate sleep duration for repeat or combo check
        let sleep_dur = if let Some(deadline) = repeat_deadline {
            let now = Instant::now();
            if deadline > now {
                deadline - now
            } else {
                Duration::ZERO
            }
        } else if passthrough_combo_start.is_some() || vt_switch_combo_start.is_some() {
            Duration::from_millis(50)
        } else {
            // No timer active - next_event will wake us
            Duration::from_secs(3600)
        };

        // Try to read next event with timeout
        let result = time::timeout(sleep_dur, stream.next_event()).await;

        match result {
            Ok(Ok(raw_event)) => {
                let Some(be) = button::from_event(&raw_event) else {
                    continue;
                };

                debug!("Event: {:?} {:?} (mode={})", be.button, be.state, mode);

                // Re-grab after returning from TTY2
                if vt_switched_away {
                    vt_switched_away = false;
                    if mode.should_grab() {
                        let _ = input::grab(stream.device_mut());
                    }
                    info!("Returned from TTY2, re-grabbed device");
                    // Clear stale state
                    held_buttons.clear();
                    mods = Modifiers::default();
                    held_button = None;
                    repeat_deadline = None;
                    continue;
                }

                // Track held buttons
                match be.state {
                    ButtonState::Pressed => {
                        held_buttons.insert(be.button);
                    }
                    ButtonState::Released => {
                        held_buttons.remove(&be.button);
                    }
                }

                // Update modifier state
                match be.button {
                    Button::L1 => mods.l1 = be.state == ButtonState::Pressed,
                    Button::R1 => mods.r1 = be.state == ButtonState::Pressed,
                    Button::F3 => mods.f3 = be.state == ButtonState::Pressed,
                    _ => {}
                }

                // Check passthrough combo (L1+R1+Start all pressed)
                if mods.l1
                    && mods.r1
                    && held_buttons.contains(&Button::Start)
                    && passthrough_combo_start.is_none()
                {
                    passthrough_combo_start = Some(Instant::now());
                    debug!("Passthrough combo started");
                }
                // If any combo key released, cancel
                if passthrough_combo_start.is_some()
                    && (!mods.l1
                        || !mods.r1
                        || !held_buttons.contains(&Button::Start))
                {
                    debug!("Passthrough combo cancelled");
                    passthrough_combo_start = None;
                }

                // Check VT switch combo (L1+R1+Select all pressed)
                if mods.l1
                    && mods.r1
                    && held_buttons.contains(&Button::F6)
                    && vt_switch_combo_start.is_none()
                {
                    vt_switch_combo_start = Some(Instant::now());
                    debug!("VT switch combo started");
                }
                // If any combo key released, cancel
                if vt_switch_combo_start.is_some()
                    && (!mods.l1
                        || !mods.r1
                        || !held_buttons.contains(&Button::F6))
                {
                    debug!("VT switch combo cancelled");
                    vt_switch_combo_start = None;
                }

                // Key repeat: track held repeatable buttons
                if is_repeatable(be.button, mode) {
                    if be.state == ButtonState::Pressed {
                        held_button = Some(be.button);
                        repeat_deadline =
                            Some(Instant::now() + Duration::from_millis(REPEAT_DELAY_MS));
                    } else if held_button == Some(be.button) {
                        held_button = None;
                        repeat_deadline = None;
                    }
                }

                // Dispatch the button event
                if mode != NavMode::GamePassthrough {
                    let action =
                        actions::dispatch(mode, be.button, be.state, &mods, &mut grid, &mut cursor).await?;
                    if let ActionResult::Transition(new_mode) = action {
                        // Clean up overlay if leaving TextEntry
                        if mode == NavMode::TextEntry && new_mode != NavMode::TextEntry {
                            grid.kill_overlay();
                            grid.reset();
                        }
                        apply_transition(stream.device_mut(), &mut mode, new_mode, &mut cursor, &mut merger)?;
                        held_button = None;
                        repeat_deadline = None;
                    }
                }
                // In passthrough, we still read events but don't dispatch
                // (the exit combo is checked in the timeout branch)
            }
            Ok(Err(e)) => {
                warn!("Error reading event: {}", e);
                time::sleep(Duration::from_millis(100)).await;
            }
            Err(_) => {
                // Timeout - check timers

                // Check passthrough combo hold duration
                if let Some(start) = passthrough_combo_start {
                    if start.elapsed() >= Duration::from_millis(PASSTHROUGH_HOLD_MS) {
                        passthrough_combo_start = None;
                        let new_mode = if mode == NavMode::GamePassthrough {
                            NavMode::WindowNav
                        } else {
                            NavMode::GamePassthrough
                        };
                        info!("Passthrough combo fired -> {}", new_mode);
                        // Clean up overlay if leaving TextEntry via passthrough
                        if mode == NavMode::TextEntry {
                            grid.kill_overlay();
                            grid.reset();
                        }
                        apply_transition(stream.device_mut(), &mut mode, new_mode, &mut cursor, &mut merger)?;
                        held_button = None;
                        repeat_deadline = None;
                    }
                }

                // Check VT switch combo hold duration (L1+R1+Select -> chvt 2)
                if let Some(start) = vt_switch_combo_start {
                    if start.elapsed() >= Duration::from_millis(VT_SWITCH_HOLD_MS) {
                        vt_switch_combo_start = None;
                        info!("VT switch combo fired -> switching to TTY2");
                        // Ungrab so TTY2 (cage/RetroArch/ES-DE) can read the gamepad
                        if mode.should_grab() {
                            let _ = input::ungrab(stream.device_mut());
                        }
                        vt_switched_away = true;
                        let _ = tokio::process::Command::new("sudo")
                            .args(["chvt", "2"])
                            .status()
                            .await;
                    }
                }

                // Software key repeat
                if let Some(btn) = held_button {
                    if repeat_deadline.map(|d| Instant::now() >= d).unwrap_or(false) {
                        debug!("Repeat: {:?}", btn);
                        let _ =
                            actions::dispatch(mode, btn, ButtonState::Pressed, &mods, &mut grid, &mut cursor).await;
                        repeat_deadline =
                            Some(Instant::now() + Duration::from_millis(REPEAT_INTERVAL_MS));
                    }
                }
            }
        }
    }
}

/// Apply a mode transition, handling grab/ungrab, cursor, and gamepad merger.
fn apply_transition(
    device: &mut evdev::Device,
    current: &mut NavMode,
    new: NavMode,
    cursor: &mut JoystickCursor,
    merger: &mut GamepadMerger,
) -> Result<()> {
    let was_grabbed = current.should_grab();
    let need_grab = new.should_grab();

    if was_grabbed && !need_grab {
        input::ungrab(device)?;
    } else if !was_grabbed && need_grab {
        input::grab(device)?;
    }

    // Pause/resume joystick cursor and activate/deactivate gamepad merger
    if new == NavMode::GamePassthrough {
        cursor.pause();
        merger.activate();
    } else if *current == NavMode::GamePassthrough {
        merger.deactivate();
        cursor.resume();
    }

    info!("Mode: {} -> {}", current, new);
    *current = new;
    Ok(())
}
