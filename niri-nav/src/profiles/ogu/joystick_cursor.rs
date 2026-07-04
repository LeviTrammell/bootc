use anyhow::Result;
use log::{info, warn};
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

/// Minimum gap between automatic respawn attempts.
const RESPAWN_COOLDOWN: Duration = Duration::from_secs(2);

fn cursor_bin() -> String {
    std::env::var("JOYSTICK_CURSOR_BIN")
        .unwrap_or_else(|_| "/usr/local/bin/joystick-cursor".into())
}

/// Manages the joystick-cursor child process that converts analog joystick
/// input into Wayland virtual pointer motion.
pub struct JoystickCursor {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    /// Whether the cursor should currently be paused; replayed to a
    /// freshly respawned child so it comes back in the right state.
    paused: bool,
    last_spawn: Option<Instant>,
}

impl JoystickCursor {
    pub fn new() -> Self {
        Self {
            child: None,
            stdin: None,
            paused: false,
            last_spawn: None,
        }
    }

    /// Spawn the joystick-cursor daemon. Inherits WAYLAND_DISPLAY from env.
    pub fn spawn(&mut self) -> Result<()> {
        self.kill();

        info!("Spawning joystick-cursor");
        self.last_spawn = Some(Instant::now());
        let mut child = Command::new(cursor_bin())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?;

        self.stdin = child.stdin.take();
        self.child = Some(child);
        if self.paused {
            self.send("PAUSE");
        }
        Ok(())
    }

    /// Respawn the child if it has exited (Wayland not up yet at boot,
    /// niri restart, crash). Called from the profile's idle tick.
    pub fn ensure_alive(&mut self) {
        let dead = match self.child {
            None => true,
            Some(ref mut c) => match c.try_wait() {
                Ok(Some(status)) => {
                    warn!("joystick-cursor exited ({status}), respawning");
                    true
                }
                Ok(None) => false,
                Err(e) => {
                    warn!("joystick-cursor status check failed: {e}");
                    false
                }
            },
        };
        if !dead {
            return;
        }
        let cooled_down = self
            .last_spawn
            .map(|t| t.elapsed() >= RESPAWN_COOLDOWN)
            .unwrap_or(true);
        if cooled_down {
            if let Err(e) = self.spawn() {
                warn!("joystick-cursor respawn failed: {e}");
            }
        }
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
        self.paused = true;
        self.send("PAUSE");
    }

    /// Resume cursor movement (e.g. leaving GamePassthrough).
    pub fn resume(&mut self) {
        self.paused = false;
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
