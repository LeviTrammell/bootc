/// Navigation modes for the OGU profile.
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
    /// controller-shell overlay (mode tiles + app launcher) is up.
    /// D-pad and face buttons are forwarded to the shell over its
    /// socket. Device is GRABBED.
    Shell,
    /// Browser mode (Zen). Analog stick drives the cursor, face
    /// buttons/triggers click, L1/R1 switch tabs, d-pad scrolls.
    /// Device is GRABBED.
    Browser,
    /// Raw passthrough for games running inside niri. Device is
    /// UNGRABBED, all input goes to apps. Daemon only watches for the
    /// exit combo.
    GamePassthrough,
    /// EmulationStation on TTY2. Like GamePassthrough (ungrabbed,
    /// merger active) but entered via chvt 2; leaving TTY2 drops back
    /// to WindowNav automatically via the VT watcher tick.
    Emulation,
}

impl NavMode {
    /// Whether the evdev device should be grabbed in this mode.
    pub fn should_grab(self) -> bool {
        match self {
            NavMode::WindowNav
            | NavMode::ElementNav
            | NavMode::TextEntry
            | NavMode::Shell
            | NavMode::Browser => true,
            NavMode::GamePassthrough | NavMode::Emulation => false,
        }
    }
}

impl std::fmt::Display for NavMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NavMode::WindowNav => write!(f, "WINDOW_NAV"),
            NavMode::ElementNav => write!(f, "ELEMENT_NAV"),
            NavMode::TextEntry => write!(f, "TEXT_ENTRY"),
            NavMode::Shell => write!(f, "SHELL"),
            NavMode::Browser => write!(f, "BROWSER"),
            NavMode::GamePassthrough => write!(f, "GAME_PASSTHROUGH"),
            NavMode::Emulation => write!(f, "EMULATION"),
        }
    }
}
