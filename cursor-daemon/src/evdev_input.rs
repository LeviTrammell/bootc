//! Gamepad evdev reader. Emits stick + button events into a channel.

use anyhow::{Context, Result};
use evdev::{AbsoluteAxisType, Device, EventType, Key};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    LeftStick(i32, i32),
    RightStick(i32, i32),
    /// (button, pressed)
    Button(MouseButton, bool),
}

#[derive(Debug, Clone, Copy)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Find a gamepad device by matching name patterns + capabilities. Same
/// heuristics as niri-nav's HTPC profile.
fn open_gamepad() -> Result<Device> {
    let devices = evdev::enumerate().collect::<Vec<_>>();
    let patterns = ["8bitdo", "x-box", "xbox", "controller", "gamepad"];
    for (path, device) in &devices {
        let name = device.name().unwrap_or("").to_lowercase();
        if patterns.iter().any(|p| name.contains(p)) {
            log::info!("Selected gamepad: {} ({})", device.name().unwrap_or("?"), path.display());
            return Device::open(path).with_context(|| format!("open {}", path.display()));
        }
    }
    for (path, device) in &devices {
        let has_south = device
            .supported_keys()
            .map(|k| k.contains(Key::BTN_SOUTH))
            .unwrap_or(false);
        let has_abs_x = device
            .supported_absolute_axes()
            .map(|a| a.contains(AbsoluteAxisType::ABS_X))
            .unwrap_or(false);
        if has_south && has_abs_x {
            log::info!("Selected by caps: {} ({})", device.name().unwrap_or("?"), path.display());
            return Device::open(path).with_context(|| format!("open {}", path.display()));
        }
    }
    anyhow::bail!("no gamepad found")
}

pub fn read_loop(tx: Sender<Event>) -> Result<()> {
    let mut device = open_gamepad()?;

    // Prime initial axis state. evdev's fetch_events() only delivers *new*
    // events, so if the stick is already deflected when we open the device
    // we'd never see motion. Query the kernel for the current absolute
    // axis state and seed our cached values from it.
    let mut lx = 0i32;
    let mut ly = 0i32;
    let mut rx_axis = 0i32;
    let mut ry_axis = 0i32;
    if let Ok(state) = device.get_abs_state() {
        let abs_x = AbsoluteAxisType::ABS_X.0 as usize;
        let abs_y = AbsoluteAxisType::ABS_Y.0 as usize;
        let abs_rx = AbsoluteAxisType::ABS_RX.0 as usize;
        let abs_ry = AbsoluteAxisType::ABS_RY.0 as usize;
        lx = state[abs_x].value;
        ly = state[abs_y].value;
        rx_axis = state[abs_rx].value;
        ry_axis = state[abs_ry].value;
        log::info!(
            "Initial stick state: left=({}, {}), right=({}, {})",
            lx, ly, rx_axis, ry_axis
        );
        let _ = tx.send(Event::LeftStick(lx, ly));
        let _ = tx.send(Event::RightStick(rx_axis, ry_axis));
    }

    loop {
        for ev in device.fetch_events()? {
            match ev.event_type() {
                EventType::ABSOLUTE => match AbsoluteAxisType(ev.code()) {
                    AbsoluteAxisType::ABS_X => {
                        lx = ev.value();
                        let _ = tx.send(Event::LeftStick(lx, ly));
                    }
                    AbsoluteAxisType::ABS_Y => {
                        ly = ev.value();
                        let _ = tx.send(Event::LeftStick(lx, ly));
                    }
                    AbsoluteAxisType::ABS_RX => {
                        rx_axis = ev.value();
                        let _ = tx.send(Event::RightStick(rx_axis, ry_axis));
                    }
                    AbsoluteAxisType::ABS_RY => {
                        ry_axis = ev.value();
                        let _ = tx.send(Event::RightStick(rx_axis, ry_axis));
                    }
                    _ => {}
                },
                EventType::KEY => {
                    let pressed = match ev.value() {
                        0 => false,
                        1 => true,
                        _ => continue,
                    };
                    let btn = match Key(ev.code()) {
                        Key::BTN_SOUTH => MouseButton::Left,
                        Key::BTN_EAST => MouseButton::Right,
                        Key::BTN_NORTH => MouseButton::Middle,
                        _ => continue,
                    };
                    let _ = tx.send(Event::Button(btn, pressed));
                }
                _ => {}
            }
        }
    }
}
