use anyhow::Result;
use log::{debug, warn};
use tokio::process::Command;

/// Execute a niri IPC action via `niri msg action`.
async fn niri_action(action: &str) -> Result<()> {
    debug!("niri: {}", action);
    let output = Command::new("niri")
        .args(["msg", "action", action])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("niri msg action {} failed: {}", action, stderr.trim());
    }
    Ok(())
}

pub async fn focus_column_left() -> Result<()> {
    niri_action("focus-column-left").await
}

pub async fn focus_column_right() -> Result<()> {
    niri_action("focus-column-right").await
}

pub async fn focus_workspace_up() -> Result<()> {
    niri_action("focus-workspace-up").await
}

pub async fn focus_workspace_down() -> Result<()> {
    niri_action("focus-workspace-down").await
}

pub async fn move_column_left() -> Result<()> {
    niri_action("move-column-left").await
}

pub async fn move_column_right() -> Result<()> {
    niri_action("move-column-right").await
}

pub async fn move_window_to_workspace_up() -> Result<()> {
    niri_action("move-window-to-workspace-up").await
}

pub async fn move_window_to_workspace_down() -> Result<()> {
    niri_action("move-window-to-workspace-down").await
}

pub async fn consume_window_into_column() -> Result<()> {
    niri_action("consume-window-into-column").await
}

pub async fn expel_window_from_column() -> Result<()> {
    niri_action("expel-window-from-column").await
}

pub async fn close_window() -> Result<()> {
    niri_action("close-window").await
}

pub async fn fullscreen_window() -> Result<()> {
    niri_action("fullscreen-window").await
}

/// Spawn a process through niri (inherits niri's WAYLAND_DISPLAY env).
pub async fn spawn(args: &[&str]) -> Result<()> {
    debug!("spawn: {:?}", args);
    let mut cmd_args = vec!["msg", "action", "spawn", "--"];
    cmd_args.extend(args);
    let output = Command::new("niri")
        .args(&cmd_args)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("niri spawn {:?} failed: {}", args, stderr.trim());
    }
    Ok(())
}
