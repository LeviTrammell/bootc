//! HTPC controller profile (Odroid H2+ / 8BitDo Ultimate via 2.4GHz dongle).
//!
//! The HTPC's UX is mode-as-foreground-app: each [`state::Mode`] roughly
//! corresponds to a top-level app the user wants on screen. The
//! [`htpc-ctl`] script owns the actual app lifecycle (start dashboard, stop
//! Pegasus, etc.). This profile is the controller-side glue:
//!
//!   - From Dashboard, HOME opens the mode picker.
//!   - From any other mode, BACK returns to Dashboard.
//!   - From Picker, A confirms (handled by the picker GUI, not here — the
//!     picker reads the gamepad directly).
//!
//! The picker, once selected, executes `htpc-ctl mode <target>` and exits;
//! this profile then receives a fresh round of events with the device
//! re-grabbed in the new mode.

pub mod button;
pub mod input;
pub mod state;

use self::state::Mode;
use crate::button::{Button, ButtonEvent, ButtonState};
use crate::modifiers::Modifiers;
use crate::profile::{ControllerProfile, Dispatch, ProfileMode};
use anyhow::Result;
use async_trait::async_trait;
use log::info;

// Resolved via PATH (the niri-nav.service runs with
// PATH=/usr/local/bin:/usr/bin) so a hot-fixed /usr/local/bin/htpc-ctl
// shadows the older copy baked into the read-only image at /usr/bin.
const HTPC_CTL: &str = "htpc-ctl";

/// Send a typed command to controller-shell. Best-effort — silent
/// failure if the shell isn't up; cs-proto's `send_shell` already
/// handles that.
fn send_shell(cmd: cs_proto::ShellCommand) {
    cs_proto::sock::send_shell(&cmd);
}

pub struct HtpcProfile {
    /// We let systemd own the cursor-daemon process via its user
    /// service; this struct just remembers that we asked for it.
    cursor_active: bool,
    /// The OSK is a modal overlay summonable from any grabbed mode.
    /// While it's up, gamepad nav events are forwarded to the shell
    /// instead of being interpreted by the active mode's dispatch.
    osk_active: bool,
}

impl HtpcProfile {
    pub fn new() -> Self {
        Self {
            cursor_active: false,
            osk_active: false,
        }
    }

    fn spawn_cursor(&mut self) {
        if self.cursor_active {
            return;
        }
        info!("starting cursor-daemon.service");
        let _ = tokio::process::Command::new("systemctl")
            .args(["--user", "start", "cursor-daemon.service"])
            .spawn();
        self.cursor_active = true;
    }

    fn kill_cursor(&mut self) {
        if !self.cursor_active {
            return;
        }
        info!("stopping cursor-daemon.service");
        let _ = tokio::process::Command::new("systemctl")
            .args(["--user", "stop", "cursor-daemon.service"])
            .spawn();
        self.cursor_active = false;
    }
}

impl Default for HtpcProfile {
    fn default() -> Self {
        Self::new()
    }
}

/// Run `htpc-ctl mode <target>` as a child process. Doesn't block on it.
async fn ctl_mode(target: &str) -> Result<()> {
    info!("htpc-ctl mode {}", target);
    let _ = tokio::process::Command::new(HTPC_CTL)
        .args(["mode", target])
        .spawn();
    Ok(())
}

#[async_trait]
impl ControllerProfile for HtpcProfile {
    type Mode = Mode;

    fn initial_mode(&self) -> Mode {
        Mode::Dashboard
    }

    fn open_gamepad(&self) -> Result<evdev::Device> {
        input::open_gamepad()
    }

    fn map_event(&self, ev: &evdev::InputEvent) -> Option<ButtonEvent> {
        button::map_event(ev)
    }

    fn is_repeatable(&self, _mode: Mode, button: Button) -> bool {
        // HTPC doesn't really need d-pad repeat — picker reads events
        // directly. But keep it for Dashboard so users can hold a d-pad
        // direction to scroll the dashboard.
        matches!(
            button,
            Button::DpadUp | Button::DpadDown | Button::DpadLeft | Button::DpadRight
        )
    }

    async fn dispatch(
        &mut self,
        mode: Mode,
        button: Button,
        state: ButtonState,
        _mods: &Modifiers,
    ) -> Result<Dispatch<Mode>> {
        if state != ButtonState::Pressed {
            return Ok(Dispatch::Stay);
        }

        // OSK is modal: while it's up, every nav button is forwarded to
        // the shell. The underlying mode (Dashboard, Picker, …) stays
        // unchanged; closing the OSK returns to it.
        if self.osk_active {
            use cs_proto::ShellCommand as SC;
            let cmd = match button {
                Button::DpadUp => Some(SC::Up),
                Button::DpadDown => Some(SC::Down),
                Button::DpadLeft => Some(SC::Left),
                Button::DpadRight => Some(SC::Right),
                Button::A => Some(SC::Select),
                Button::B => Some(SC::Back),
                Button::X => Some(SC::Keyboard), // toggles shift/symbols layers
                Button::Y => Some(SC::Options),
                _ => None,
            };
            if let Some(c) = cmd {
                send_shell(c.clone());
                // B in OSK = close. Shell hides itself; mirror state here.
                if matches!(c, SC::Back) {
                    self.osk_active = false;
                }
                return Ok(Dispatch::Stay);
            }
        }

        // While the picker is up, forward d-pad + A/B/X/Y to controller-shell.
        if mode == Mode::Picker {
            use cs_proto::ShellCommand as SC;
            // Shoulder buttons switch between the picker's views: R1 opens
            // the app drawer ("apps"), L1 returns to the mode tiles
            // ("modes"). This is the controller equivalent of the dev
            // keyboard's Tab; without it the app drawer is unreachable on a
            // pad. switch_view is idempotent shell-side, so a redundant
            // press is harmless.
            match button {
                Button::R1 => {
                    send_shell(SC::View {
                        name: "apps".into(),
                    });
                    return Ok(Dispatch::Stay);
                }
                Button::L1 => {
                    send_shell(SC::View {
                        name: "modes".into(),
                    });
                    return Ok(Dispatch::Stay);
                }
                _ => {}
            }
            let cmd = match button {
                Button::DpadUp => Some(SC::Up),
                Button::DpadDown => Some(SC::Down),
                Button::DpadLeft => Some(SC::Left),
                Button::DpadRight => Some(SC::Right),
                Button::A => Some(SC::Select),
                Button::B => Some(SC::Back),
                Button::X => Some(SC::Keyboard),
                Button::Y => Some(SC::Options),
                _ => None,
            };
            if let Some(c) = cmd {
                let was_keyboard = matches!(c, SC::Keyboard);
                send_shell(c);
                if was_keyboard {
                    self.osk_active = true;
                }
                // HOME / BACK close the picker entirely (handled below).
                if !matches!(button, Button::B) {
                    return Ok(Dispatch::Stay);
                }
            }
        }

        // L2 in any grabbed mode summons the OSK. Useful when you're on
        // the Dashboard with a chromium kiosk text input focused and want
        // to type without a keyboard.
        if button == Button::L2 && !self.osk_active && mode.should_grab() {
            send_shell(cs_proto::ShellCommand::Keyboard);
            self.osk_active = true;
            return Ok(Dispatch::Stay);
        }

        // Button::Select == BACK on Xbox-style 8BitDo (BTN_SELECT 314).
        match (mode, button) {
            // HOME opens the picker from any grabbed mode.
            (Mode::Dashboard | Mode::Desktop, Button::Home) => {
                return Ok(Dispatch::Transition(Mode::Picker));
            }
            // From Picker, HOME or BACK closes the picker back to Dashboard.
            (Mode::Picker, Button::Home | Button::Select | Button::B) => {
                return Ok(Dispatch::Transition(Mode::Dashboard));
            }
            // BACK from any grabbed mode → Dashboard (via htpc-ctl).
            (_, Button::Select) => {
                ctl_mode("dashboard").await?;
                return Ok(Dispatch::Transition(Mode::Dashboard));
            }
            _ => {}
        }

        Ok(Dispatch::Stay)
    }

    async fn on_transition(&mut self, from: Mode, to: Mode) -> Result<()> {
        info!("Mode: {} -> {}", from, to);
        // cursor-daemon lifecycle is bound to Desktop mode. Anywhere else,
        // it's killed so it doesn't fight Pegasus / Steam / etc. for the
        // gamepad.
        if to == Mode::Desktop && from != Mode::Desktop {
            self.spawn_cursor();
        } else if from == Mode::Desktop && to != Mode::Desktop {
            self.kill_cursor();
        }
        // controller-shell visibility tracks Picker mode: show on entry,
        // hide on exit.
        if to == Mode::Picker && from != Mode::Picker {
            send_shell(cs_proto::ShellCommand::Show);
        } else if from == Mode::Picker && to != Mode::Picker {
            send_shell(cs_proto::ShellCommand::Hide);
        }
        // Escaping out of a gameplay mode (the L1+R1+Start passthrough
        // combo path takes you Pegasus → GamePassthrough → Dashboard, or
        // direct via IPC) needs to actually KILL whatever's running.
        // Shell out to htpc-ctl mode dashboard which runs stop_foreground.
        let was_gameplay = matches!(
            from,
            Mode::Pegasus
                | Mode::Kodi
                | Mode::Steam
                | Mode::Moonlight
                | Mode::RetroArch
                | Mode::GamePassthrough
        );
        if was_gameplay && to == Mode::Dashboard {
            info!("escaping gameplay -> dashboard; spawning htpc-ctl mode dashboard");
            let _ = tokio::process::Command::new(HTPC_CTL)
                .args(["mode", "dashboard"])
                .spawn();
        }
        Ok(())
    }

    fn toggle_passthrough(&self, _current: Mode) -> Mode {
        // The L1+R1+Start universal combo on HTPC means "get me out of
        // whatever I'm in and back to the dashboard, killing the game if
        // there is one". The OGU profile uses a true toggle (grabbed ↔
        // passthrough), but HTPC's gameplay modes are already ungrabbed
        // so the intermediate `GamePassthrough` step was just an extra
        // hold for no behaviour change. Go straight to Dashboard from
        // anywhere; the on_transition hook shells out to htpc-ctl which
        // kills whatever foreground app was running.
        Mode::Dashboard
    }

    fn mode_from_name(&self, name: &str) -> Option<Mode> {
        Some(match name {
            "dashboard" => Mode::Dashboard,
            "pegasus" => Mode::Pegasus,
            "kodi" => Mode::Kodi,
            "steam" => Mode::Steam,
            "moonlight" => Mode::Moonlight,
            "retroarch" => Mode::RetroArch,
            "desktop" => Mode::Desktop,
            "picker" => Mode::Picker,
            "passthrough" | "game" => Mode::GamePassthrough,
            _ => return None,
        })
    }
}

