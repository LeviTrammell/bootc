//! Unix-socket server for receiving nav commands from niri-nav.
//!
//! Bound at `$XDG_RUNTIME_DIR/controller-shell.sock`. Accepts one JSON
//! object per line (newline-delimited so multiple commands can be
//! pipelined on one connection). Commands are the same as the stdin
//! protocol in `ipc.rs` — Show / Hide / Up / Down / Left / Right /
//! Select / Back / Options / Keyboard / View.
//!
//! Runs on a background thread; parsed commands are pushed into a
//! std::sync::mpsc channel that gtk's main loop drains on a timer.

use crate::ipc::{self, Command};
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

pub fn spawn(tx: Sender<Command>) -> anyhow::Result<()> {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    log::info!("controller-shell IPC listening on {}", path.display());

    thread::spawn(move || loop {
        let (stream, _) = match listener.accept() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("ipc accept: {e}");
                continue;
            }
        };
        let tx = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(cmd) = ipc::parse(&line) {
                    if tx.send(cmd).is_err() {
                        break;
                    }
                }
            }
        });
    });
    Ok(())
}

fn socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    PathBuf::from(dir).join("controller-shell.sock")
}
