use anyhow::Result;
use log::{debug, warn};
use tokio::process::Command;

// Linux key codes (from linux/input-event-codes.h)
const KEY_ESC: u32 = 1;
const KEY_ENTER: u32 = 28;
const KEY_UP: u32 = 103;
const KEY_LEFT: u32 = 105;
const KEY_RIGHT: u32 = 106;
const KEY_DOWN: u32 = 108;
const KEY_SPACE: u32 = 57;

/// Send a key press+release via ydotool (routes through uinput,
/// reaches layer-shell surfaces unlike wtype).
async fn ydotool_key(code: u32) -> Result<()> {
    debug!("ydotool: key {}", code);
    let output = Command::new("ydotool")
        .args(["key", &format!("{}:1", code), &format!("{}:0", code)])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("ydotool key {} failed: {}", code, stderr.trim());
    }
    Ok(())
}

pub async fn up() -> Result<()> {
    ydotool_key(KEY_UP).await
}

pub async fn down() -> Result<()> {
    ydotool_key(KEY_DOWN).await
}

pub async fn left() -> Result<()> {
    ydotool_key(KEY_LEFT).await
}

pub async fn right() -> Result<()> {
    ydotool_key(KEY_RIGHT).await
}

pub async fn enter() -> Result<()> {
    ydotool_key(KEY_ENTER).await
}

pub async fn escape() -> Result<()> {
    ydotool_key(KEY_ESC).await
}

pub async fn space() -> Result<()> {
    ydotool_key(KEY_SPACE).await
}
