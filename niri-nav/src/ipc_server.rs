//! Tiny Unix-socket server so outside processes (the controller-shell
//! mode picker, the HA shell-command bridge, `htpc-ctl`) can drive
//! niri-nav's mode state directly without going through gamepad events.
//!
//! Wire types come from [`cs_proto::NavCommand`] so the protocol is
//! shared with any other nav adapter (hyprland-nav, sway-nav, ...).

use cs_proto::NavCommand;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

/// Internal command type — keeps the runtime decoupled from the wire
/// representation so we can grow it without exposing serde at the
/// runtime boundary.
#[derive(Debug, Clone)]
pub enum IpcCommand {
    SetMode(String),
}

/// Bind `$XDG_RUNTIME_DIR/niri-nav.sock` and spawn a task that pushes
/// incoming commands to the runtime via the channel.
pub fn spawn(tx: mpsc::UnboundedSender<IpcCommand>) -> anyhow::Result<()> {
    let path = cs_proto::sock::runtime_path("niri-nav.sock");
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    log::info!("niri-nav IPC listening on {}", path.display());
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("ipc accept: {e}");
                    continue;
                }
            };
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut buf = Vec::with_capacity(256);
                if stream.read_to_end(&mut buf).await.is_err() {
                    return;
                }
                let line = String::from_utf8_lossy(&buf);
                if let Some(cmd) = cs_proto::parse::<NavCommand>(&line) {
                    match cmd {
                        NavCommand::Mode { name } => {
                            let _ = tx.send(IpcCommand::SetMode(name));
                        }
                    }
                }
            });
        }
    });
    Ok(())
}
