use anyhow::Result;
use log::{info, warn};
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

/// Manages the gamepad-merger child process that merges three evdev devices
/// (left stick, right stick, gpio-keys) into one virtual uinput gamepad.
pub struct GamepadMerger {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

impl GamepadMerger {
    pub fn new() -> Self {
        Self {
            child: None,
            stdin: None,
        }
    }

    /// Spawn the gamepad-merger daemon.
    pub fn spawn(&mut self) -> Result<()> {
        self.kill();

        // Only spawn if /dev/uinput exists
        if !std::path::Path::new("/dev/uinput").exists() {
            warn!("gamepad-merger: /dev/uinput not available, skipping");
            return Ok(());
        }

        info!("Spawning gamepad-merger");
        let mut child = Command::new("/usr/local/bin/gamepad-merger")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;

        self.stdin = child.stdin.take();
        self.child = Some(child);
        Ok(())
    }

    fn send(&mut self, cmd: &str) {
        if let Some(ref mut stdin) = self.stdin {
            let msg = format!("{}\n", cmd);
            if let Err(e) = stdin.write_all(msg.as_bytes()) {
                warn!("Failed to send '{}' to gamepad-merger: {}", cmd, e);
                self.kill();
            } else {
                let _ = stdin.flush();
            }
        }
    }

    /// Activate event forwarding (entering GamePassthrough).
    pub fn activate(&mut self) {
        self.send("ACTIVATE");
    }

    /// Deactivate event forwarding (leaving GamePassthrough).
    pub fn deactivate(&mut self) {
        self.send("DEACTIVATE");
    }

    /// Kill the daemon and drop handles.
    pub fn kill(&mut self) {
        if let Some(ref mut child) = self.child {
            info!("Killing gamepad-merger");
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stdin = None;
        self.child = None;
    }
}

impl Drop for GamepadMerger {
    fn drop(&mut self) {
        self.kill();
    }
}
