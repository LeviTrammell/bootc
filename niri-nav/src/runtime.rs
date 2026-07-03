//! Profile-agnostic event-loop runtime.

use crate::button::{Button, ButtonState};
use crate::input;
use crate::ipc_server::{self, IpcCommand};
use crate::modifiers::Modifiers;
use crate::profile::{ControllerProfile, Dispatch, ProfileMode};
use tokio::sync::mpsc;
use anyhow::Result;
use log::{debug, info, warn};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::time;

const PASSTHROUGH_HOLD_MS: u64 = 500;
const REPEAT_DELAY_MS: u64 = 400;
const REPEAT_INTERVAL_MS: u64 = 150;

const PASS_KEYS: &[Button] = &[Button::L1, Button::R1, Button::Start];

pub async fn run<P: ControllerProfile>(mut profile: P) -> Result<()> {
    info!("niri-nav starting");
    ensure_wayland_display();

    let device = profile.open_gamepad()?;
    let mut stream = device
        .into_event_stream()
        .map_err(|e| anyhow::anyhow!("Failed to create event stream: {}", e))?;

    // Restore the mode from a previous run if one was persisted this
    // session. This stops a mid-game niri-nav restart (the 8BitDo's
    // power-save sleep trips the read-error bail, then systemd Restart=
    // brings us back) from slamming into the grabbed Dashboard mode and
    // stealing the pad from whatever game is in the foreground.
    let mut mode = match restore_mode(&profile) {
        Some(m) => {
            info!("Restored mode from previous run: {}", m);
            m
        }
        None => profile.initial_mode(),
    };
    let mut mods = Modifiers::default();
    let mut held: HashSet<Button> = HashSet::new();

    if mode.should_grab() {
        if let Err(e) = input::grab(stream.device_mut()) {
            warn!("initial grab failed (continuing): {e}");
        }
    }
    info!("Mode: {}", mode);
    save_mode(mode);
    profile.on_start(mode).await?;

    let mut pass_start: Option<Instant> = None;
    let extra_combo = profile.extra_combo();
    let mut extra_start: Option<Instant> = None;
    let mut held_button: Option<Button> = None;
    let mut repeat_deadline: Option<Instant> = None;
    // Track consecutive read errors so we can detect the gamepad
    // having gone away (typical for the 8BitDo's power-save sleep).
    // After ~1s of solid ENODEV we exit; systemd Restart=always brings
    // us back up, our open_gamepad() then picks up the new event node
    // the kernel assigned to the reconnected device.
    let mut read_err_streak: u32 = 0;
    const READ_ERR_BAIL_THRESHOLD: u32 = 10;

    // Spawn Unix-socket IPC server so the mode picker / htpc-ctl can
    // drive mode transitions out-of-band from gamepad events.
    let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel::<IpcCommand>();
    if let Err(e) = ipc_server::spawn(ipc_tx) {
        log::warn!("ipc server: {e} (continuing without IPC)");
    }

    loop {
        // Drain IPC commands before reading the next gamepad event.
        while let Ok(cmd) = ipc_rx.try_recv() {
            match cmd {
                IpcCommand::SetMode(name) => {
                    if let Some(nm) = profile.mode_from_name(&name) {
                        if nm != mode {
                            log::info!("ipc: mode -> {}", nm);
                            mode = apply_transition(&mut profile, &mut stream, mode, nm).await?;
                            held_button = None;
                            repeat_deadline = None;
                        }
                    } else {
                        log::warn!("ipc: unknown mode name {name:?}");
                    }
                }
            }
        }

        let sleep_dur = if let Some(d) = repeat_deadline {
            d.saturating_duration_since(Instant::now())
        } else if pass_start.is_some() || extra_start.is_some() {
            Duration::from_millis(50)
        } else {
            // Wake periodically so IPC commands arrive within ~250ms even
            // if no gamepad input.
            Duration::from_millis(250)
        };

        match time::timeout(sleep_dur, stream.next_event()).await {
            Ok(Ok(raw)) => {
                read_err_streak = 0;
                let Some(be) = profile.map_event(&raw) else {
                    continue;
                };

                debug!("Event: {:?} {:?} (mode={})", be.button, be.state, mode);

                match be.state {
                    ButtonState::Pressed => {
                        held.insert(be.button);
                    }
                    ButtonState::Released => {
                        held.remove(&be.button);
                    }
                }
                mods.update(be.button, be.state == ButtonState::Pressed);

                update_combo_timer(&mut pass_start, PASS_KEYS, &held, "passthrough");
                if let Some((keys, _hold)) = extra_combo {
                    update_combo_timer(&mut extra_start, keys, &held, "extra");
                }

                if profile.is_repeatable(mode, be.button) {
                    if be.state == ButtonState::Pressed {
                        held_button = Some(be.button);
                        repeat_deadline =
                            Some(Instant::now() + Duration::from_millis(REPEAT_DELAY_MS));
                    } else if held_button == Some(be.button) {
                        held_button = None;
                        repeat_deadline = None;
                    }
                }

                if mode.should_dispatch() {
                    let action = profile.dispatch(mode, be.button, be.state, &mods).await?;
                    if let Dispatch::Transition(nm) = action {
                        mode = apply_transition(&mut profile, &mut stream, mode, nm).await?;
                        held_button = None;
                        repeat_deadline = None;
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("Error reading event: {}", e);
                time::sleep(Duration::from_millis(100)).await;
                read_err_streak += 1;
                if read_err_streak >= READ_ERR_BAIL_THRESHOLD {
                    anyhow::bail!(
                        "gamepad device has been unreadable for {} consecutive ticks ({} ms) — \
                         likely disconnected. Exiting so systemd can restart us and re-scan for \
                         the device on its new event node.",
                        read_err_streak, read_err_streak * 100,
                    );
                }
            }
            Err(_) => {
                // Passthrough combo (universal).
                if let Some(start) = pass_start {
                    if start.elapsed() >= Duration::from_millis(PASSTHROUGH_HOLD_MS) {
                        pass_start = None;
                        let nm = profile.toggle_passthrough(mode);
                        info!("Passthrough combo fired");
                        mode = apply_transition(&mut profile, &mut stream, mode, nm).await?;
                        held_button = None;
                        repeat_deadline = None;
                    }
                }

                // Profile-specific extra combo (e.g. OGU VT switch).
                if let (Some(start), Some((_, hold))) = (extra_start, extra_combo) {
                    if start.elapsed() >= Duration::from_millis(hold) {
                        extra_start = None;
                        info!("Extra combo fired");
                        let maybe_nm = profile.on_extra_combo(mode).await?;
                        if let Some(nm) = maybe_nm {
                            mode = apply_transition(&mut profile, &mut stream, mode, nm).await?;
                            held_button = None;
                            repeat_deadline = None;
                        }
                    }
                }

                if let Some(btn) = held_button {
                    if repeat_deadline.map(|d| Instant::now() >= d).unwrap_or(false) {
                        debug!("Repeat: {:?}", btn);
                        let _ = profile.dispatch(mode, btn, ButtonState::Pressed, &mods).await;
                        repeat_deadline =
                            Some(Instant::now() + Duration::from_millis(REPEAT_INTERVAL_MS));
                    }
                }

                // Profile idle hook (e.g. OGU's VT watcher).
                if let Some(nm) = profile.tick(mode).await? {
                    if nm != mode {
                        info!("tick: mode -> {}", nm);
                        mode = apply_transition(&mut profile, &mut stream, mode, nm).await?;
                        held_button = None;
                        repeat_deadline = None;
                    }
                }
            }
        }
    }
}

/// Path to the persisted-mode file in the session runtime dir. Lives in
/// XDG_RUNTIME_DIR (tmpfs), so it's wiped when the session ends — a fresh
/// login always starts from the profile's `initial_mode()`, while a
/// same-session service restart restores where we left off.
fn mode_state_path() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| std::path::PathBuf::from(d).join("niri-nav.mode"))
}

/// Write the current mode so a restart can pick it back up. Best-effort:
/// a write failure just means we fall back to `initial_mode()` next start.
fn save_mode<M: ProfileMode>(mode: M) {
    if let Some(path) = mode_state_path() {
        // `Display` is the canonical name; lowercased it round-trips
        // through the profile's `mode_from_name()` on restore.
        let _ = std::fs::write(&path, mode.to_string());
    }
}

/// Read back a persisted mode, parsing it via the profile's name table.
/// None if there's no file, it's unreadable, or the name is unknown (e.g.
/// a profile change) — caller falls back to `initial_mode()`.
fn restore_mode<P: ControllerProfile>(profile: &P) -> Option<P::Mode> {
    let raw = std::fs::read_to_string(mode_state_path()?).ok()?;
    profile.mode_from_name(raw.trim().to_lowercase().as_str())
}

fn update_combo_timer(
    slot: &mut Option<Instant>,
    keys: &[Button],
    held: &HashSet<Button>,
    name: &str,
) {
    let all_held = keys.iter().all(|b| held.contains(b));
    if all_held && slot.is_none() {
        *slot = Some(Instant::now());
        debug!("{} combo started", name);
    } else if !all_held && slot.is_some() {
        debug!("{} combo cancelled", name);
        *slot = None;
    }
}

async fn apply_transition<P: ControllerProfile>(
    profile: &mut P,
    stream: &mut evdev::EventStream,
    from: P::Mode,
    to: P::Mode,
) -> Result<P::Mode> {
    profile.on_transition(from, to).await?;
    // Persist as soon as the target is known so a restart restores it.
    save_mode(to);
    if from.should_grab() != to.should_grab() {
        // grab/ungrab can fail with ENODEV when the gamepad has gone to
        // sleep / disconnected. We log + continue rather than crash —
        // the device will reappear (or systemd will restart us if not).
        if to.should_grab() {
            if let Err(e) = input::grab(stream.device_mut()) {
                warn!("grab failed (continuing): {e}");
            }
        } else if let Err(e) = input::ungrab(stream.device_mut()) {
            warn!("ungrab failed (continuing): {e}");
        }
    }
    Ok(to)
}

fn ensure_wayland_display() {
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return;
    }
    let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&runtime_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("wayland-") && !name.ends_with(".lock") {
            info!("Auto-detected WAYLAND_DISPLAY={}", name);
            std::env::set_var("WAYLAND_DISPLAY", &*name);
            return;
        }
    }
}
