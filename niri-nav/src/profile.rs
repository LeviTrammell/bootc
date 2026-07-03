//! The `ControllerProfile` trait — pluggable per-device behavior.
//!
//! The runtime in [`crate::runtime`] is profile-agnostic; it owns the event
//! loop, grab/ungrab, modifier tracking, key-repeat, and the universal
//! L1+R1+Start passthrough combo. Profiles plug in:
//!   - the evdev device to grab as the gamepad
//!   - the raw-code → logical-button table
//!   - the set of modes and dispatch (what each button does in each mode)
//!   - which modes auto-repeat which buttons
//!   - which mode is the passthrough target
//!   - optionally a second combo (e.g. OGU's L1+R1+Select → chvt 2)

use crate::button::{Button, ButtonEvent, ButtonState};
use crate::modifiers::Modifiers;
use anyhow::Result;
use async_trait::async_trait;
use std::fmt;

#[derive(Debug)]
pub enum Dispatch<M> {
    Stay,
    Transition(M),
}

pub trait ProfileMode: Copy + Eq + fmt::Debug + fmt::Display + Send + Sync + 'static {
    /// Whether the evdev device should be grabbed (EVIOCGRAB) in this mode.
    fn should_grab(self) -> bool;

    /// Whether per-event [`ControllerProfile::dispatch`] should fire in
    /// this mode. Defaults to `should_grab()`. Override for modes where a
    /// secondary daemon (e.g. cursor-daemon) reads the device but the
    /// nav profile still wants to see button presses for things like HOME
    /// → mode picker.
    fn should_dispatch(self) -> bool {
        self.should_grab()
    }
}

#[async_trait]
pub trait ControllerProfile: Send + 'static {
    type Mode: ProfileMode;

    fn initial_mode(&self) -> Self::Mode;
    fn open_gamepad(&self) -> Result<evdev::Device>;
    fn map_event(&self, ev: &evdev::InputEvent) -> Option<ButtonEvent>;
    fn is_repeatable(&self, mode: Self::Mode, button: Button) -> bool;

    async fn dispatch(
        &mut self,
        mode: Self::Mode,
        button: Button,
        state: ButtonState,
        mods: &Modifiers,
    ) -> Result<Dispatch<Self::Mode>>;

    /// Translate an external mode name (received over IPC) into the
    /// profile's mode enum. None for unknown names. Profiles wire this
    /// to the names that `htpc-ctl mode <name>` and controller-shell
    /// use.
    fn mode_from_name(&self, _name: &str) -> Option<Self::Mode> {
        None
    }

    /// Apply L1+R1+Start (passthrough combo). Profiles return the mode to
    /// switch to: typically toggle between their passthrough target and the
    /// "home" mode.
    fn toggle_passthrough(&self, current: Self::Mode) -> Self::Mode;

    /// Optional secondary combo. None = no second combo.
    /// Returns (keys, hold_ms).
    fn extra_combo(&self) -> Option<(&'static [Button], u64)> {
        None
    }

    /// Called when the secondary combo fires. Free to perform side effects
    /// (chvt, spawn process, etc.). Return Some(mode) to also transition.
    async fn on_extra_combo(&mut self, _current: Self::Mode) -> Result<Option<Self::Mode>> {
        Ok(None)
    }

    /// Hook fired before applying a mode change. Profiles use this to open
    /// or close overlays, pause/resume helper daemons, etc.
    async fn on_transition(&mut self, _from: Self::Mode, _to: Self::Mode) -> Result<()> {
        Ok(())
    }

    /// Called once at startup after the initial mode is in effect (grab
    /// applied, no on_transition fired). Profiles that pair external
    /// state with a mode (e.g. OGU shows controller-shell when booting
    /// into Shell) sync it here.
    async fn on_start(&mut self, _mode: Self::Mode) -> Result<()> {
        Ok(())
    }

    /// Periodic idle hook (fires on the runtime's ~250ms event-loop
    /// ticks). Return Some(mode) to transition. OGU uses this to notice
    /// the console switched back to the niri VT while in Emulation mode
    /// and re-enter WindowNav automatically.
    async fn tick(&mut self, _current: Self::Mode) -> Result<Option<Self::Mode>> {
        Ok(None)
    }
}
