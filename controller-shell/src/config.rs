//! Per-variant runtime configuration.
//!
//! The shell itself is variant-agnostic; what differs between the HTPC
//! and the OGU (and any future variant) is which tiles the modes row
//! shows and how control is handed back to the nav adapter. That lives
//! in a JSON file baked into each image:
//!
//!   1. `$CONTROLLER_SHELL_CONFIG` (dev override)
//!   2. `/etc/controller-shell/config.json`
//!   3. built-in HTPC defaults (so existing HTPC images keep working
//!      without a config file)

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    /// Tiles on the modes row, in display order.
    pub tiles: Vec<Tile>,
    /// Nav mode sent to the adapter after launching an app from the
    /// Apps view (so the adapter ungrabs / sets up the right cursor
    /// state for the launched app).
    pub app_launch_nav_mode: String,
    /// Nav mode sent when Back dismisses the shell from the root view.
    /// None = just hide; the nav adapter transitions on its own (the
    /// HTPC profile drives Picker → Dashboard itself).
    pub exit_nav_mode: Option<String>,
    /// Launch apps through `niri msg action spawn` so niri owns the
    /// child process instead of the shell. Keeps launched apps alive
    /// across shell restarts.
    pub launch_via_niri: bool,
    /// Command prefix used to wrap `Terminal=true` desktop entries
    /// (e.g. ["alacritty", "-e"]). None = launch them directly and
    /// hope for the best (old behavior).
    pub terminal_prefix: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Tile {
    pub key: String,
    pub label: String,
    /// freedesktop icon name; gtk4 IconTheme resolves.
    pub icon: String,
    /// `NavCommand::Mode` name sent to the nav adapter on select.
    #[serde(default)]
    pub nav_mode: Option<String>,
    /// argv spawned on select (e.g. ["htpc-ctl", "mode", "kodi"]).
    #[serde(default)]
    pub exec: Option<Vec<String>>,
    /// Shell view to switch to instead of dismissing ("apps"). A tile
    /// with `view` set ignores `nav_mode`/`exec`.
    #[serde(default)]
    pub view: Option<String>,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            tiles: htpc_default_tiles(),
            app_launch_nav_mode: "desktop".into(),
            exit_nav_mode: None,
            launch_via_niri: false,
            terminal_prefix: None,
        }
    }
}

pub fn load() -> ShellConfig {
    let path = std::env::var("CONTROLLER_SHELL_CONFIG")
        .unwrap_or_else(|_| "/etc/controller-shell/config.json".into());
    match std::fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(cfg) => {
                log::info!("config loaded from {path}");
                cfg
            }
            Err(e) => {
                log::error!("config {path} is invalid ({e}); using built-in defaults");
                ShellConfig::default()
            }
        },
        Err(_) => {
            log::info!("no config at {path}; using built-in HTPC defaults");
            ShellConfig::default()
        }
    }
}

/// The pre-config hardcoded HTPC tile set: each tile both tells the nav
/// adapter the mode name and shells out to htpc-ctl for app lifecycle.
fn htpc_default_tiles() -> Vec<Tile> {
    fn t(key: &str, label: &str, icon: &str) -> Tile {
        Tile {
            key: key.into(),
            label: label.into(),
            icon: icon.into(),
            nav_mode: Some(key.into()),
            exec: Some(vec!["htpc-ctl".into(), "mode".into(), key.into()]),
            view: None,
        }
    }
    vec![
        t("dashboard", "Dashboard", "view-grid-symbolic"),
        t("pegasus", "Games", "applications-games-symbolic"),
        t("kodi", "Movies", "applications-multimedia-symbolic"),
        t("steam", "Steam", "applications-engineering-symbolic"),
        t("moonlight", "Stream", "network-wireless-symbolic"),
        t("retroarch", "RetroArch", "applications-games-symbolic"),
        t("desktop", "Desktop", "preferences-desktop-symbolic"),
    ]
}
