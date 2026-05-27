use anyhow::Result;
use log::{info, warn};
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

/// Manages the joystick-cursor child process that converts analog joystick
/// input into Wayland virtual pointer motion.
pub struct JoystickCursor {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

impl JoystickCursor {
    pub fn new() -> Self {
        Self {
            child: None,
            stdin: None,
        }
    }

    /// Spawn the joystick-cursor daemon. Inherits WAYLAND_DISPLAY from env.
    pub fn spawn(&mut self) -> Result<()> {
        self.kill();

        info!("Spawning joystick-cursor");
        let mut child = Command::new("/usr/local/bin/joystick-cursor")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;

        self.stdin = child.stdin.take();
        self.child = Some(child);
        Ok(())
    }

    /// Send a command to the joystick-cursor daemon via stdin pipe.
    fn send(&mut self, cmd: &str) {
        if let Some(ref mut stdin) = self.stdin {
            let msg = format!("{}\n", cmd);
            if let Err(e) = stdin.write_all(msg.as_bytes()) {
                warn!("Failed to send '{}' to joystick-cursor: {}", cmd, e);
                self.kill();
            } else {
                let _ = stdin.flush();
            }
        }
    }

    /// Send a left mouse click.
    pub fn click_left(&mut self) {
        self.send("CLICK left");
    }

    /// Send a right mouse click.
    pub fn click_right(&mut self) {
        self.send("CLICK right");
    }

    /// Pause cursor movement (e.g. entering GamePassthrough).
    pub fn pause(&mut self) {
        self.send("PAUSE");
    }

    /// Resume cursor movement (e.g. leaving GamePassthrough).
    pub fn resume(&mut self) {
        self.send("RESUME");
    }

    /// Kill the daemon and drop handles.
    pub fn kill(&mut self) {
        if let Some(ref mut child) = self.child {
            info!("Killing joystick-cursor");
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stdin = None;
        self.child = None;
    }
}

impl Drop for JoystickCursor {
    fn drop(&mut self) {
        self.kill();
    }
}
