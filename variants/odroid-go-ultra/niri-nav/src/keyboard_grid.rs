use anyhow::Result;
use log::{debug, info, warn};
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

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

pub struct KeyboardGrid {
    pub row: usize,
    pub col: usize,
    pub shift: bool,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
}

impl KeyboardGrid {
    pub fn new() -> Self {
        Self {
            row: 1,
            col: 0,
            shift: false,
            child: None,
            stdin: None,
        }
    }

    fn layout(&self) -> &'static [&'static [char]] {
        if self.shift { SHIFTED } else { NORMAL }
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

    pub fn toggle_shift(&mut self) {
        self.shift = !self.shift;
        debug!("Shift: {}", self.shift);
    }

    /// Reset cursor to default position (for when overlay is dismissed).
    pub fn reset(&mut self) {
        self.row = 1;
        self.col = 0;
        self.shift = false;
    }

    /// Spawn the osk-overlay process and pipe stdin for position updates.
    pub fn spawn_overlay(&mut self) -> Result<()> {
        // Kill any existing overlay first
        self.kill_overlay();

        info!("Spawning osk-overlay");
        let mut child = Command::new("/usr/local/bin/osk-overlay")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        self.stdin = child.stdin.take();
        self.child = Some(child);

        // Send initial position
        self.send_position();
        Ok(())
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
    }

    /// Send current position to the overlay via stdin pipe.
    pub fn send_position(&mut self) {
        let shift_val: u8 = if self.shift { 1 } else { 0 };
        let msg = format!("POS {} {} {}\n", self.row, self.col, shift_val);
        if let Some(ref mut stdin) = self.stdin {
            if let Err(e) = stdin.write_all(msg.as_bytes()) {
                warn!("Failed to send position to overlay: {}", e);
                // Overlay probably died, clean up
                self.kill_overlay();
            } else {
                let _ = stdin.flush();
            }
        }
    }
}
