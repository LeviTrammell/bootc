//! Unix socket helpers — connect/send + listen/accept. Used by both the
//! shell (server side, listens) and the nav adapter (server side too;
//! plus client side when forwarding events).
//!
//! Paths live in `$XDG_RUNTIME_DIR` so they inherit the user-private
//! `0700` directory perms.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

/// Default name for the controller-shell socket.
pub const SHELL_SOCK_NAME: &str = "controller-shell.sock";

/// Build a path of the form `$XDG_RUNTIME_DIR/<name>`.
pub fn runtime_path(name: &str) -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    PathBuf::from(dir).join(name)
}

/// Fire-and-forget: connect to a unix socket, write a payload, close.
/// Returns false silently if the socket isn't there — the caller is
/// expected to be tolerant of the other end being absent.
pub fn send(name: &str, payload: &str) -> bool {
    let path = runtime_path(name);
    match UnixStream::connect(&path) {
        Ok(mut s) => {
            let _ = s.write_all(payload.as_bytes());
            true
        }
        Err(e) => {
            log::debug!("cs-proto send {}: {e}", path.display());
            false
        }
    }
}

/// Send a `cmd-as-JSON-line` to the controller-shell. Convenience wrapper.
pub fn send_shell<T: serde::Serialize>(cmd: &T) -> bool {
    let line = match crate::line(cmd) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("cs-proto encode: {e}");
            return false;
        }
    };
    send(SHELL_SOCK_NAME, &line)
}

/// Send a `cmd-as-JSON-line` to the named nav adapter (e.g.
/// `"niri-nav.sock"`).
pub fn send_nav<T: serde::Serialize>(sock_name: &str, cmd: &T) -> bool {
    let line = match crate::line(cmd) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("cs-proto encode: {e}");
            return false;
        }
    };
    send(sock_name, &line)
}
