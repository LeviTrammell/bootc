//! `.desktop` discovery + icon resolution.
//!
//! Walks the freedesktop application directories, filters out hidden /
//! NoDisplay entries, strips Exec field codes, and resolves the Icon=
//! string to an absolute path the gtk4 Image widget can render.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub icon_path: Option<PathBuf>,
    pub icon_name: Option<String>,
    pub exec: Vec<String>,
    /// `Terminal=true` — needs to be wrapped in a terminal emulator.
    pub terminal: bool,
}

/// Standard XDG application directories, in lookup order.
fn application_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/applications"));
    }
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for d in xdg_data_dirs.split(':') {
        if d.is_empty() {
            continue;
        }
        dirs.push(PathBuf::from(d).join("applications"));
    }
    // Flatpak exports go through XDG_DATA_DIRS already on most distros,
    // but list explicitly in case they're not advertised.
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs
}

/// Discover all visible app entries. Deduplicates by file stem (later
/// entries override earlier ones — matches XDG precedence: user > system).
pub fn discover() -> Vec<AppEntry> {
    use std::collections::HashMap;
    let mut by_id: HashMap<String, AppEntry> = HashMap::new();

    for dir in application_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let id = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if let Some(parsed) = parse_desktop_file(&path) {
                by_id.insert(id, parsed);
            }
        }
    }

    let mut entries: Vec<AppEntry> = by_id.into_values().collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    entries
}

fn parse_desktop_file(path: &std::path::Path) -> Option<AppEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_main = false;
    let mut name: Option<String> = None;
    let mut icon: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut no_display = false;
    let mut hidden = false;
    let mut entry_type: Option<String> = None;
    let mut terminal = false;

    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim();
        match key {
            "Name" if name.is_none() => name = Some(val.to_string()),
            "Icon" if icon.is_none() => icon = Some(val.to_string()),
            "Exec" if exec.is_none() => exec = Some(val.to_string()),
            "NoDisplay" => no_display = val.eq_ignore_ascii_case("true"),
            "Hidden" => hidden = val.eq_ignore_ascii_case("true"),
            "Type" => entry_type = Some(val.to_string()),
            "Terminal" => terminal = val.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    // Filter rules from the freedesktop Desktop Entry spec.
    if no_display || hidden {
        return None;
    }
    if entry_type.as_deref() != Some("Application") {
        return None;
    }

    let name = name?;
    let exec_str = exec?;
    let exec = strip_field_codes(&exec_str);
    if exec.is_empty() {
        return None;
    }

    let (icon_path, icon_name) = match icon {
        Some(i) if i.starts_with('/') => (Some(PathBuf::from(&i)), None),
        Some(i) => (None, Some(i)),
        None => (None, None),
    };

    Some(AppEntry {
        name,
        icon_path,
        icon_name,
        exec,
        terminal,
    })
}

/// Tokenise an Exec= line, removing freedesktop field codes (%f %F %u %U
/// %d %D %n %N %i %c %k). Doesn't honour quoting beyond simple splitting,
/// which is good enough for the launch path (we shell out via Command).
fn strip_field_codes(exec: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in exec.split_whitespace() {
        if token.len() == 2 && token.starts_with('%') {
            continue;
        }
        out.push(token.to_string());
    }
    out
}

/// Spawn the app, fire-and-forget.
///
/// `Terminal=true` entries are wrapped with the configured terminal
/// prefix (e.g. `alacritty -e`). With `launch_via_niri`, the command
/// goes through `niri msg action spawn` so niri (not the shell) owns
/// the child — launched apps survive a shell restart.
pub fn launch(entry: &AppEntry, config: &crate::config::ShellConfig) {
    if entry.exec.is_empty() {
        return;
    }
    let mut argv: Vec<String> = Vec::new();
    if entry.terminal {
        match &config.terminal_prefix {
            Some(prefix) => argv.extend(prefix.iter().cloned()),
            None => log::warn!(
                "{}: Terminal=true but no terminal_prefix configured; launching bare",
                entry.name
            ),
        }
    }
    argv.extend(entry.exec.iter().cloned());
    if config.launch_via_niri {
        let mut spawn = vec![
            "msg".to_string(),
            "action".to_string(),
            "spawn".to_string(),
            "--".to_string(),
        ];
        spawn.extend(argv);
        argv = spawn;
        argv.insert(0, "niri".to_string());
    }
    log::info!("launching {} ({:?})", entry.name, argv);
    let _ = Command::new(&argv[0]).args(&argv[1..]).spawn();
}
