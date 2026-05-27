/// Navigation modes for the niri-nav daemon.
///
/// The daemon is always in exactly one mode. Mode determines how gamepad
/// buttons are interpreted and whether the evdev device is grabbed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavMode {
    /// Default mode. D-pad navigates niri columns/workspaces.
    /// Device is GRABBED.
    WindowNav,
    /// Element navigation inside a window. D-pad cycles focusable
    /// elements (Tab/Shift+Tab) and arrows. Device is GRABBED.
    ElementNav,
    /// On-screen keyboard visible. D-pad navigates OSK grid.
    /// Device is GRABBED.
    TextEntry,
    /// App launcher overlay (nwg-drawer). D-pad sends arrows,
    /// X selects, B/Y dismisses. Device is GRABBED.
    Launcher,
    /// Raw passthrough. Device is UNGRABBED, all input goes to apps.
    /// Daemon only watches for exit combo.
    GamePassthrough,
}

impl NavMode {
    /// Whether the evdev device should be grabbed in this mode.
    pub fn should_grab(self) -> bool {
        match self {
            NavMode::WindowNav | NavMode::ElementNav | NavMode::TextEntry | NavMode::Launcher => {
                true
            }
            NavMode::GamePassthrough => false,
        }
    }
}

impl std::fmt::Display for NavMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NavMode::WindowNav => write!(f, "WINDOW_NAV"),
            NavMode::ElementNav => write!(f, "ELEMENT_NAV"),
            NavMode::TextEntry => write!(f, "TEXT_ENTRY"),
            NavMode::Launcher => write!(f, "LAUNCHER"),
            NavMode::GamePassthrough => write!(f, "GAME_PASSTHROUGH"),
        }
    }
}
