//! Thin wrapper around `zwlr_virtual_pointer_manager_v1`.
//!
//! Connects to Wayland, binds the manager + seat from the registry,
//! and creates a single virtual pointer that the main loop drives.

use crate::evdev_input::MouseButton;
use anyhow::{anyhow, Result};
use std::time::{SystemTime, UNIX_EPOCH};
use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::{
        wl_pointer::{Axis, ButtonState},
        wl_registry::WlRegistry,
        wl_seat::WlSeat,
    },
    Connection, Dispatch, EventQueue, QueueHandle,
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

// Linux input event button codes (from <linux/input-event-codes.h>).
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;


/// Event sink for the queue — all the protocols we bind are fire-only
/// from our side, so every Dispatch impl is a no-op.
struct State;

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerManagerV1,
        _: <ZwlrVirtualPointerManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrVirtualPointerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrVirtualPointerV1,
        _: <ZwlrVirtualPointerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: <WlSeat as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

pub struct VirtualPointer {
    queue: EventQueue<State>,
    pointer: ZwlrVirtualPointerV1,
}

impl VirtualPointer {
    pub fn connect() -> Result<Self> {
        let conn = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
        let qh = queue.handle();

        let manager: ZwlrVirtualPointerManagerV1 = globals
            .bind(&qh, 1..=2, ())
            .map_err(|e| anyhow!("compositor doesn't expose zwlr_virtual_pointer_manager_v1: {e}"))?;
        // Any wl_seat will do; we don't pick a specific one.
        let seat: WlSeat = globals
            .bind(&qh, 1..=9, ())
            .map_err(|e| anyhow!("no wl_seat: {e}"))?;
        let pointer: ZwlrVirtualPointerV1 = manager.create_virtual_pointer(Some(&seat), &qh, ());
        // Drop the manager — we only need the pointer from here on.
        drop(manager);

        let mut state = State;
        queue.roundtrip(&mut state)?;

        Ok(Self { queue, pointer })
    }

    pub fn motion(&mut self, dx: f64, dy: f64) -> Result<()> {
        self.pointer.motion(time_ms(), dx, dy);
        self.flush()
    }

    pub fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        let t = time_ms();
        if dy != 0.0 {
            self.pointer.axis(t, Axis::VerticalScroll, dy);
        }
        if dx != 0.0 {
            self.pointer.axis(t, Axis::HorizontalScroll, dx);
        }
        self.flush()
    }

    pub fn button(&mut self, button: MouseButton, pressed: bool) -> Result<()> {
        let code = match button {
            MouseButton::Left => BTN_LEFT,
            MouseButton::Right => BTN_RIGHT,
            MouseButton::Middle => BTN_MIDDLE,
        };
        let state = if pressed {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        };
        self.pointer.button(time_ms(), code, state);
        self.flush()
    }

    pub fn frame(&mut self) -> Result<()> {
        self.pointer.frame();
        self.flush()
    }

    fn flush(&mut self) -> Result<()> {
        self.queue.flush()?;
        Ok(())
    }
}

fn time_ms() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(0)
}
