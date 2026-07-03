//! Wire protocol + socket helpers shared by controller-shell, niri-nav,
//! and any future compositor adapter (hyprland-nav, sway-nav, ...).
//!
//! The protocol is intentionally compositor-agnostic — mode names are
//! strings, not a sealed enum, so each nav adapter can define its own
//! mode set without churning this crate.
//!
//! ## Sockets
//!
//! Two well-known sockets in `$XDG_RUNTIME_DIR`:
//!
//!   - `controller-shell.sock` — bound by controller-shell; accepts
//!     [`ShellCommand`]s from nav adapters.
//!   - `<adapter>-nav.sock` — bound by the active nav adapter (e.g.
//!     `niri-nav.sock`); accepts [`NavCommand`]s from controller-shell
//!     or `htpc-ctl`.
//!
//! Connections are one JSON object per line, then close. Senders are
//! fire-and-forget: drift is preferred over locking up the producer if
//! the consumer is missing.

pub mod sock;

use serde::{Deserialize, Serialize};

/// Commands the controller-shell accepts on its socket. Sent by the
/// active nav adapter when forwarding gamepad events, and by anyone
/// (HA, scripts, dev tools) for direct control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ShellCommand {
    Show,
    Hide,
    Up,
    Down,
    Left,
    Right,
    Select,
    Back,
    /// Triangle/Y — context menu for the focused tile.
    Options,
    /// Square/X — summon on-screen keyboard.
    Keyboard,
    /// Switch to a named view ("modes", "apps", ...). The shell decides
    /// which views it implements.
    View {
        name: String,
    },
}

/// Commands the nav adapter accepts on its socket. The shell sends one
/// of these when the user picks a mode tile; htpc-ctl translates its
/// own `mode <name>` verb into one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum NavCommand {
    /// Switch to a named mode. Adapter-specific names —
    /// `mode_from_name()` on the active controller-profile does the
    /// translation.
    Mode {
        name: String,
    },
}

/// Serialise a value to a single JSON line (newline-terminated).
pub fn line<T: Serialize>(value: &T) -> anyhow::Result<String> {
    let mut s = serde_json::to_string(value)?;
    s.push('\n');
    Ok(s)
}

/// Parse one JSON line into the target type. Whitespace tolerant.
pub fn parse<T: for<'de> Deserialize<'de>>(line: &str) -> Option<T> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    match serde_json::from_str(line) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("cs-proto parse: {e} (line={line:?})");
            None
        }
    }
}
