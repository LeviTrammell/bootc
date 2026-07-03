//! Odroid Go Ultra controller profile.
//!
//! Handheld with on-board gpio-keys gamepad, dual analog sticks (separate
//! evdev devices merged via gamepad-merger), 480x854 portrait MIPI panel
//! rotated to landscape. No mouse — analog stick drives a virtual pointer
//! via joystick-cursor. No physical keyboard — OSK overlay used for text.
//!
//! The console-style UX is: boot into the controller-shell overlay
//! (Shell mode), pick a tile — EmulationStation switches to TTY2
//! (Emulation mode), apps launch into niri (WindowNav). Start summons
//! the shell from anywhere in niri.

pub mod actions;
pub mod button;
pub mod gamepad_merger;
pub mod input;
pub mod joystick_cursor;
pub mod keyboard_grid;
pub mod niri;
pub mod state;
pub mod wtype;

use self::actions::ActionResult;
use self::gamepad_merger::GamepadMerger;
use self::joystick_cursor::JoystickCursor;
use self::keyboard_grid::KeyboardGrid;
use self::state::NavMode;

use crate::button::{Button, ButtonEvent, ButtonState};
use crate::modifiers::Modifiers;
use crate::profile::{ControllerProfile, Dispatch, ProfileMode};
use anyhow::Result;
use async_trait::async_trait;
use log::{info, warn};
use std::time::{Duration, Instant};

/// How long after entering Emulation mode before the VT watcher may pull
/// us back to WindowNav — covers the gap between the transition and the
/// `chvt 2` actually taking effect.
const VT_WATCH_GRACE: Duration = Duration::from_millis(1500);

impl ProfileMode for NavMode {
    fn should_grab(self) -> bool {
        NavMode::should_grab(self)
    }
}

/// Per-frame state owned by the OGU profile (not the runtime).
pub struct OguProfile {
    cursor: JoystickCursor,
    merger: GamepadMerger,
    grid: KeyboardGrid,
    /// F3 is an OGU-only modifier; tracked here rather than in shared
    /// [`Modifiers`] which only carries universal keys.
    f3: bool,
    /// When Emulation mode was entered — gates the VT watcher.
    emulation_since: Option<Instant>,
}

fn send_shell(cmd: cs_proto::ShellCommand) {
    cs_proto::sock::send_shell(&cmd);
}

/// Currently active virtual console number, from sysfs ("tty1" → 1).
fn active_vt() -> Option<u32> {
    let raw = std::fs::read_to_string("/sys/class/tty/tty0/active").ok()?;
    raw.trim().strip_prefix("tty")?.parse().ok()
}

impl OguProfile {
    pub fn new() -> Self {
        let mut cursor = JoystickCursor::new();
        if let Err(e) = cursor.spawn() {
            warn!(
                "Failed to spawn joystick-cursor: {} (cursor control disabled)",
                e
            );
        }

        let mut merger = GamepadMerger::new();
        if let Err(e) = merger.spawn() {
            warn!(
                "Failed to spawn gamepad-merger: {} (gamepad passthrough disabled)",
                e
            );
        }

        Self {
            cursor,
            merger,
            grid: KeyboardGrid::new(),
            f3: false,
            emulation_since: None,
        }
    }
}

impl Default for OguProfile {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ControllerProfile for OguProfile {
    type Mode = NavMode;

    fn initial_mode(&self) -> NavMode {
        // Boot into the console home screen, PSP-style. The
        // controller-shell service starts visible to match.
        NavMode::Shell
    }

    fn open_gamepad(&self) -> Result<evdev::Device> {
        input::open_gamepad()
    }

    fn map_event(&self, ev: &evdev::InputEvent) -> Option<ButtonEvent> {
        button::map_event(ev)
    }

    fn is_repeatable(&self, mode: NavMode, button: Button) -> bool {
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

    async fn dispatch(
        &mut self,
        mode: NavMode,
        button: Button,
        state: ButtonState,
        mods: &Modifiers,
    ) -> Result<Dispatch<NavMode>> {
        // OGU-only modifier kept on the profile.
        if button == Button::F3 {
            self.f3 = state == ButtonState::Pressed;
        }

        let res = actions::dispatch(
            mode,
            button,
            state,
            mods,
            self.f3,
            &mut self.grid,
            &mut self.cursor,
        )
        .await?;
        Ok(match res {
            ActionResult::Stay => Dispatch::Stay,
            ActionResult::Transition(m) => Dispatch::Transition(m),
        })
    }

    async fn on_start(&mut self, mode: NavMode) -> Result<()> {
        // Booting (or restarting) into Shell mode: make sure the overlay
        // is actually on screen. Idempotent if it already is.
        if mode == NavMode::Shell {
            send_shell(cs_proto::ShellCommand::Show);
        }
        if mode == NavMode::Emulation {
            self.emulation_since = Some(Instant::now());
        }
        Ok(())
    }

    async fn on_transition(&mut self, from: NavMode, to: NavMode) -> Result<()> {
        // Leaving TextEntry: tear down OSK overlay.
        if from == NavMode::TextEntry && to != NavMode::TextEntry {
            self.grid.kill_overlay();
            self.grid.reset();
        }

        // controller-shell visibility tracks Shell mode.
        if to == NavMode::Shell && from != NavMode::Shell {
            send_shell(cs_proto::ShellCommand::Show);
        } else if from == NavMode::Shell && to != NavMode::Shell {
            send_shell(cs_proto::ShellCommand::Hide);
        }

        // Entering an ungrabbed gaming mode: pause cursor, activate
        // merger so the virtual gamepad sees events. Reverse on exit.
        let gaming = |m: NavMode| matches!(m, NavMode::GamePassthrough | NavMode::Emulation);
        if gaming(to) && !gaming(from) {
            self.cursor.pause();
            self.merger.activate();
        } else if gaming(from) && !gaming(to) {
            self.merger.deactivate();
            self.cursor.resume();
        }

        // Emulation lives on TTY2; the VT watcher in tick() brings us
        // back to WindowNav once the console returns to TTY1 (via
        // vt-switch-monitor's L1+R1+F6, or ES-DE exiting).
        if to == NavMode::Emulation {
            self.emulation_since = Some(Instant::now());
            info!("switching to TTY2 (EmulationStation)");
            let _ = tokio::process::Command::new("sudo")
                .args(["chvt", "2"])
                .status()
                .await;
        } else {
            self.emulation_since = None;
        }

        info!("Mode: {} -> {}", from, to);
        Ok(())
    }

    async fn tick(&mut self, current: NavMode) -> Result<Option<NavMode>> {
        // VT watcher: while in Emulation the user is on TTY2. When the
        // console comes back to TTY1 (vt-switch-monitor combo or ES-DE
        // exit), re-enter WindowNav so the pad drives niri again without
        // needing another combo.
        if current == NavMode::Emulation {
            let past_grace = self
                .emulation_since
                .map(|t| t.elapsed() >= VT_WATCH_GRACE)
                .unwrap_or(true);
            if past_grace && active_vt() == Some(1) {
                info!("console back on TTY1 -> WINDOW_NAV");
                return Ok(Some(NavMode::WindowNav));
            }
        }
        Ok(None)
    }

    fn toggle_passthrough(&self, current: NavMode) -> NavMode {
        // Never react to the combo while the console is on another VT
        // (TTY2 emulation) — regrabbing there would steal the pad from
        // the emulator. The VT watcher handles the return trip.
        if active_vt().map(|vt| vt != 1).unwrap_or(false) {
            return current;
        }
        match current {
            NavMode::GamePassthrough | NavMode::Emulation => NavMode::WindowNav,
            _ => NavMode::GamePassthrough,
        }
    }

    fn mode_from_name(&self, name: &str) -> Option<NavMode> {
        Some(match name {
            "window_nav" | "windows" => NavMode::WindowNav,
            "element_nav" => NavMode::ElementNav,
            "text_entry" => NavMode::TextEntry,
            "shell" | "picker" => NavMode::Shell,
            "game_passthrough" | "passthrough" | "game" => NavMode::GamePassthrough,
            "emulation" => NavMode::Emulation,
            _ => return None,
        })
    }

    fn extra_combo(&self) -> Option<(&'static [Button], u64)> {
        // L1+R1+F6 held for 500ms → jump straight to TTY2 emulation.
        // Same combo vt-switch-monitor watches on the TTY2 side, so it
        // acts as a hardware-ish VT toggle.
        Some((&[Button::L1, Button::R1, Button::F6], 500))
    }

    async fn on_extra_combo(&mut self, current: NavMode) -> Result<Option<NavMode>> {
        // On TTY2 the same combo is vt-switch-monitor's "back to TTY1"
        // trigger — both daemons see the ungrabbed pad. Stand down and
        // let it chvt 1; our VT watcher then restores WindowNav.
        if current == NavMode::Emulation || active_vt().map(|vt| vt != 1).unwrap_or(false) {
            return Ok(None);
        }
        info!("VT switch combo fired -> EMULATION");
        // chvt + merger handling happen in on_transition.
        Ok(Some(NavMode::Emulation))
    }
}
