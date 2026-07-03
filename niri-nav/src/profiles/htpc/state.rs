//! HTPC modes — built around mode-as-foreground-app rather than OGU's
//! window-navigation grammar.

use crate::profile::ProfileMode;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Chromium kiosk showing the HA dashboard. Device grabbed so HOME
    /// pulls up the mode picker overlay.
    Dashboard,
    /// Pegasus frontend (gamescope-wrapped). Device ungrabbed — Pegasus
    /// owns gamepad input.
    Pegasus,
    /// Kodi / Steam / Moonlight / RetroArch — all "an app owns the
    /// screen". Device ungrabbed.
    Kodi,
    Steam,
    Moonlight,
    RetroArch,
    /// Bare niri, nothing in foreground. Device grabbed.
    Desktop,
    /// Mode picker overlay shown (htpc-mode-picker). Device grabbed.
    Picker,
    /// Raw passthrough — same as Pegasus/etc. but used as the
    /// runtime-universal passthrough target when the L1+R1+Start combo
    /// fires from any mode.
    GamePassthrough,
}

impl ProfileMode for Mode {
    fn should_grab(self) -> bool {
        match self {
            Mode::Dashboard | Mode::Picker => true,
            // Desktop is *ungrabbed* so cursor-daemon can read the gamepad
            // alongside us. We still dispatch (see should_dispatch) so HOME
            // and the passthrough combo still work.
            Mode::Desktop
            | Mode::Pegasus
            | Mode::Kodi
            | Mode::Steam
            | Mode::Moonlight
            | Mode::RetroArch
            | Mode::GamePassthrough => false,
        }
    }

    fn should_dispatch(self) -> bool {
        match self {
            // Desktop wants face-button dispatch (HOME → Picker) even
            // though the device isn't grabbed, so cursor-daemon can also
            // see motion events.
            Mode::Dashboard | Mode::Picker | Mode::Desktop => true,
            Mode::Pegasus
            | Mode::Kodi
            | Mode::Steam
            | Mode::Moonlight
            | Mode::RetroArch
            | Mode::GamePassthrough => false,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Dashboard => write!(f, "DASHBOARD"),
            Mode::Pegasus => write!(f, "PEGASUS"),
            Mode::Kodi => write!(f, "KODI"),
            Mode::Steam => write!(f, "STEAM"),
            Mode::Moonlight => write!(f, "MOONLIGHT"),
            Mode::RetroArch => write!(f, "RETROARCH"),
            Mode::Desktop => write!(f, "DESKTOP"),
            Mode::Picker => write!(f, "PICKER"),
            Mode::GamePassthrough => write!(f, "PASSTHROUGH"),
        }
    }
}
