use anyhow::Result;
use log::{debug, info, warn};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

/// QWERTY keyboard grid layout.
/// Row 0: 10 keys (numbers/symbols)
/// Row 1: 10 keys (qwerty top row)
/// Row 2: 9 keys (home row)
/// Row 3: 10 keys (bottom row + punctuation)
const NORMAL: &[&[char]] = &[
    &['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
    &['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p'],
    &['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l'],
    &['z', 'x', 'c', 'v', 'b', 'n', 'm', '.', '-', '/'],
];

const SHIFTED: &[&[char]] = &[
    &['!', '@', '#', '$', '%', '^', '&', '*', '(', ')'],
    &['Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P'],
    &['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L'],
    &['Z', 'X', 'C', 'V', 'B', 'N', 'M', ':', '_', '?'],
];

/// URL/symbol layer: everything a URL needs on the top row, digits on
/// the bottom so ports don't require a layer switch.
const SYMBOLS: &[&[char]] = &[
    &[':', '/', '?', '.', '-', '_', '~', '=', '&', '+'],
    &['@', '#', '$', '%', '^', '*', '(', ')', '[', ']'],
    &['\'', '"', '`', ';', ',', '!', '<', '>', '\\'],
    &['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
];

/// Minimum gap between automatic overlay respawn attempts.
const RESPAWN_COOLDOWN: Duration = Duration::from_secs(2);

fn overlay_bin() -> String {
    std::env::var("OSK_OVERLAY_BIN").unwrap_or_else(|_| "/usr/local/bin/osk-overlay".into())
}

/// Focus events reported by the osk-overlay child on its stdout,
/// sourced from zwp_input_method_v2 (the compositor's text-field focus
/// tracking — what makes the OSK pop up PSP/phone-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OskEvent {
    /// A text field gained focus; the overlay has shown itself.
    Activated,
    /// The text field lost focus; the overlay has hidden itself.
    Deactivated,
    /// input-method-v2 is unavailable (another IM client owns the
    /// seat, or not a Wayland session). Manual SHOW/HIDE still works.
    Unavailable,
}

pub struct KeyboardGrid {
    pub row: usize,
    pub col: usize,
    /// Active layout layer: 0 = lowercase, 1 = shifted, 2 = symbols.
    pub layer: usize,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    events: Option<Receiver<OskEvent>>,
    last_spawn: Option<Instant>,
}

impl KeyboardGrid {
    pub fn new() -> Self {
        Self {
            row: 1,
            col: 0,
            layer: 0,
            child: None,
            stdin: None,
            events: None,
            last_spawn: None,
        }
    }

    fn layout(&self) -> &'static [&'static [char]] {
        match self.layer {
            1 => SHIFTED,
            2 => SYMBOLS,
            _ => NORMAL,
        }
    }

    fn row_len(&self, row: usize) -> usize {
        self.layout()[row].len()
    }

    /// Get the character at the current cursor position.
    pub fn current_char(&self) -> String {
        self.layout()[self.row][self.col].to_string()
    }

    pub fn move_up(&mut self) {
        if self.row == 0 {
            self.row = self.layout().len() - 1;
        } else {
            self.row -= 1;
        }
        // Clamp col to new row length
        let max_col = self.row_len(self.row) - 1;
        if self.col > max_col {
            self.col = max_col;
        }
    }

    pub fn move_down(&mut self) {
        self.row += 1;
        if self.row >= self.layout().len() {
            self.row = 0;
        }
        // Clamp col to new row length
        let max_col = self.row_len(self.row) - 1;
        if self.col > max_col {
            self.col = max_col;
        }
    }

    pub fn move_left(&mut self) {
        if self.col == 0 {
            self.col = self.row_len(self.row) - 1;
        } else {
            self.col -= 1;
        }
    }

    pub fn move_right(&mut self) {
        self.col += 1;
        if self.col >= self.row_len(self.row) {
            self.col = 0;
        }
    }

    /// Cycle lowercase -> shifted -> symbols -> lowercase.
    pub fn cycle_layer(&mut self) {
        self.layer = (self.layer + 1) % 3;
        // Clamp col: row 2 is one key shorter on every layer, but stay
        // safe if layouts ever diverge.
        let max_col = self.row_len(self.row) - 1;
        if self.col > max_col {
            self.col = max_col;
        }
        debug!("Layer: {}", self.layer);
    }

    /// Reset cursor to default position (for when overlay is dismissed).
    pub fn reset(&mut self) {
        self.row = 1;
        self.col = 0;
        self.layer = 0;
    }

    /// Spawn the persistent osk-overlay child. It starts hidden and
    /// shows itself on IM activation or a SHOW command.
    pub fn spawn_overlay(&mut self) -> Result<()> {
        self.kill_overlay();

        info!("Spawning osk-overlay");
        self.last_spawn = Some(Instant::now());
        let mut child = Command::new(overlay_bin())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        self.stdin = child.stdin.take();

        // Reader thread: overlay stdout lines -> OskEvent channel. The
        // thread ends on EOF when the child dies.
        let stdout = child.stdout.take();
        let (tx, rx) = std::sync::mpsc::channel();
        if let Some(stdout) = stdout {
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    let ev = match line.trim() {
                        "IM ACTIVE" => OskEvent::Activated,
                        "IM INACTIVE" => OskEvent::Deactivated,
                        "IM UNAVAILABLE" => OskEvent::Unavailable,
                        _ => continue,
                    };
                    if tx.send(ev).is_err() {
                        break;
                    }
                }
            });
        }
        self.events = Some(rx);
        self.child = Some(child);

        // Send initial position
        self.send_position();
        Ok(())
    }

    /// Respawn the overlay if it has exited (lost the Wayland race at
    /// boot, compositor restart, crash). Called from the profile tick.
    pub fn ensure_alive(&mut self) {
        let dead = match self.child {
            None => true,
            Some(ref mut c) => match c.try_wait() {
                Ok(Some(status)) => {
                    warn!("osk-overlay exited ({status}), respawning");
                    true
                }
                Ok(None) => false,
                Err(e) => {
                    warn!("osk-overlay status check failed: {e}");
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
            if let Err(e) = self.spawn_overlay() {
                warn!("osk-overlay respawn failed: {e}");
            }
        }
    }

    /// Drain pending focus events from the overlay.
    pub fn poll_events(&mut self) -> Vec<OskEvent> {
        let mut out = Vec::new();
        if let Some(rx) = &self.events {
            loop {
                match rx.try_recv() {
                    Ok(ev) => out.push(ev),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.events = None;
                        break;
                    }
                }
            }
        }
        out
    }

    /// Show the overlay (manual summon — IM activation shows it on its own).
    pub fn show_overlay(&mut self) {
        self.send_line("SHOW");
        self.send_position();
    }

    /// Hide the overlay.
    pub fn hide_overlay(&mut self) {
        self.send_line("HIDE");
    }

    /// Kill the overlay process and drop handles.
    pub fn kill_overlay(&mut self) {
        if let Some(ref mut child) = self.child {
            debug!("Killing osk-overlay");
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stdin = None;
        self.child = None;
        self.events = None;
    }

    fn send_line(&mut self, line: &str) {
        if let Some(ref mut stdin) = self.stdin {
            if let Err(e) = stdin.write_all(format!("{}\n", line).as_bytes()) {
                warn!("Failed to send {} to overlay: {}", line, e);
                // Overlay probably died; ensure_alive will respawn it.
                self.kill_overlay();
            } else {
                let _ = stdin.flush();
            }
        }
    }

    /// Send current position to the overlay via stdin pipe.
    pub fn send_position(&mut self) {
        let msg = format!("POS {} {} {}", self.row, self.col, self.layer);
        self.send_line(&msg);
    }
}
