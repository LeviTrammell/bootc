//! cursor-daemon — analog stick → Wayland virtual pointer.
//!
//! Reads the gamepad over evdev. Left stick drives the cursor;
//! right stick drives scroll wheel. A button (BTN_SOUTH) = left
//! click, B (BTN_EAST) = right click, X (BTN_NORTH) = middle.
//!
//! Binds `zwlr_virtual_pointer_manager_v1` once at startup and emits
//! relative motion events every ~16ms (60Hz) while the stick is
//! deflected past a deadzone.
//!
//! This is the Rust port of OGU's `joystick-cursor.c`; the protocol +
//! semantics are intentionally identical so HTPC and OGU can share
//! upstream behaviour.

mod evdev_input;
mod virtual_pointer;

use anyhow::Result;
use log::{info, warn};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Polling rate for the motion loop. 60Hz feels right at TV distance.
const TICK_HZ: u64 = 60;
const TICK_DUR: Duration = Duration::from_micros(1_000_000 / TICK_HZ);

/// Joystick centre deadzone; values within ±DEADZONE map to zero motion.
const DEADZONE: i32 = 4_000;
/// Max raw absolute value reported by typical pads (range -32768..32767).
const STICK_MAX: i32 = 32_767;
/// Pixels per second at full deflection. Tuned for 1080p; couch users
/// can drop this in config later if it's too snappy.
const MAX_SPEED_PX_PER_S: f32 = 1100.0;
/// Maximum scroll units per second at full deflection of right stick.
const MAX_SCROLL_UNITS_PER_S: f32 = 15.0;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("cursor-daemon starting");

    let mut pointer = virtual_pointer::VirtualPointer::connect()?;
    info!("virtual pointer bound");

    let (tx, rx) = mpsc::channel::<evdev_input::Event>();
    thread::spawn(move || {
        if let Err(e) = evdev_input::read_loop(tx) {
            warn!("evdev reader exited: {e}");
        }
    });

    let mut lx: i32 = 0;
    let mut ly: i32 = 0;
    let mut rx_axis: i32 = 0;
    let mut ry_axis: i32 = 0;

    let mut last = Instant::now();
    // Once-per-second motion debug accumulator.
    let mut dbg_ticks: u32 = 0;
    let (mut dbg_dx, mut dbg_dy) = (0.0f32, 0.0f32);
    loop {
        // Drain pending events without blocking.
        loop {
            match rx.try_recv() {
                Ok(evdev_input::Event::LeftStick(x, y)) => {
                    lx = apply_deadzone(x);
                    ly = apply_deadzone(y);
                }
                Ok(evdev_input::Event::RightStick(x, y)) => {
                    rx_axis = apply_deadzone(x);
                    ry_axis = apply_deadzone(y);
                }
                Ok(evdev_input::Event::Button(btn, pressed)) => {
                    if let Err(e) = pointer.button(btn, pressed) {
                        warn!("button: {e}");
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    warn!("evdev channel closed; exiting");
                    return Ok(());
                }
            }
        }

        let now = Instant::now();
        let dt = (now - last).as_secs_f32();
        last = now;

        {
            // Motion: convert stick deflection in [-1.0, 1.0] into px/sec,
            // then to delta pixels for this tick.
            let nx = lx as f32 / STICK_MAX as f32;
            let ny = ly as f32 / STICK_MAX as f32;
            // Apply a mild curve so small deflections are precise but
            // full-tilt is fast.
            let dx = curve(nx) * MAX_SPEED_PX_PER_S * dt;
            let dy = curve(ny) * MAX_SPEED_PX_PER_S * dt;
            if dx.abs() > 0.0 || dy.abs() > 0.0 {
                if let Err(e) = pointer.motion(dx as f64, dy as f64) {
                    warn!("motion: {e}");
                }
            }
            // Once-per-second motion log so we can see whether deltas are
            // being computed but not visibly applied.
            dbg_ticks += 1;
            dbg_dx += dx;
            dbg_dy += dy;
            if dbg_ticks >= TICK_HZ as u32 {
                log::debug!(
                    "motion last 1s: dx_total={dbg_dx:.1}px dy_total={dbg_dy:.1}px (lx={lx} ly={ly})"
                );
                dbg_ticks = 0;
                dbg_dx = 0.0;
                dbg_dy = 0.0;
            }

            // Scroll: right stick → wheel.
            let snx = rx_axis as f32 / STICK_MAX as f32;
            let sny = ry_axis as f32 / STICK_MAX as f32;
            let sx = snx * MAX_SCROLL_UNITS_PER_S * dt;
            let sy = sny * MAX_SCROLL_UNITS_PER_S * dt;
            if sx.abs() > 0.0 || sy.abs() > 0.0 {
                if let Err(e) = pointer.scroll(sx as f64, sy as f64) {
                    warn!("scroll: {e}");
                }
            }
        }

        // Frame: tell the compositor "this batch of events is done".
        if let Err(e) = pointer.frame() {
            warn!("frame: {e}");
        }

        // Pace at ~60Hz. Sleep less if we've already used the budget.
        let elapsed = now.elapsed();
        if let Some(sleep) = TICK_DUR.checked_sub(elapsed) {
            thread::sleep(sleep);
        }
    }
}

fn apply_deadzone(v: i32) -> i32 {
    if v.abs() < DEADZONE {
        0
    } else {
        // Rescale so the edge of the deadzone maps to ~0 motion rather
        // than a sudden jump.
        let sign = v.signum();
        sign * (v.abs() - DEADZONE) * STICK_MAX / (STICK_MAX - DEADZONE)
    }
}

fn curve(x: f32) -> f32 {
    // Quadratic with sign preservation: x * |x|. Smooth, easy to tune.
    x * x.abs()
}
