use anyhow::Result;
use log::{debug, warn};
use tokio::process::Command;

/// Send a keystroke via wtype.
async fn wtype_key(key: &str) -> Result<()> {
    debug!("wtype: {}", key);
    let output = Command::new("wtype")
        .args(["-k", key])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("wtype -k {} failed: {}", key, stderr.trim());
    }
    Ok(())
}

/// Send a modified keystroke (e.g. Shift+Tab).
async fn wtype_modified(modifier: &str, key: &str) -> Result<()> {
    debug!("wtype: {}+{}", modifier, key);
    let output = Command::new("wtype")
        .args(["-M", modifier, "-k", key, "-m", modifier])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("wtype {}+{} failed: {}", modifier, key, stderr.trim());
    }
    Ok(())
}

// --- Navigation keys ---

pub async fn tab() -> Result<()> {
    wtype_key("Tab").await
}

pub async fn shift_tab() -> Result<()> {
    wtype_modified("shift", "Tab").await
}

pub async fn left() -> Result<()> {
    wtype_key("Left").await
}

pub async fn right() -> Result<()> {
    wtype_key("Right").await
}

pub async fn enter() -> Result<()> {
    wtype_key("Return").await
}

pub async fn space() -> Result<()> {
    wtype_key("space").await
}

pub async fn escape() -> Result<()> {
    wtype_key("Escape").await
}

pub async fn backspace() -> Result<()> {
    wtype_key("BackSpace").await
}

/// Type a literal text string (not a key name).
pub async fn type_text(text: &str) -> Result<()> {
    debug!("wtype text: {}", text);
    let output = Command::new("wtype")
        .arg(text)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("wtype '{}' failed: {}", text, stderr.trim());
    }
    Ok(())
}

// --- Extended navigation (L1+D-pad in InWindow) ---

pub async fn page_up() -> Result<()> {
    wtype_key("Prior").await
}

pub async fn page_down() -> Result<()> {
    wtype_key("Next").await
}

pub async fn home() -> Result<()> {
    wtype_key("Home").await
}

pub async fn end() -> Result<()> {
    wtype_key("End").await
}
