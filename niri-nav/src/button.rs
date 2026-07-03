//! Logical buttons used by the navigation daemon.
//!
//! These are profile-agnostic names. Each [`crate::profile::ControllerProfile`]
//! maps its hardware's raw evdev codes into these logical values via
//! [`crate::profile::ControllerProfile::map_event`].
//!
//! Not every controller has every button — e.g. the OGU has F1–F6 silkscreened
//! buttons used as Start/Select/etc., the 8BitDo Ultimate has none of them but
//! does have a `Home` (Xbox/PS Guide) button. Profiles ignore buttons that
//! don't exist on their hardware.

// Not every profile constructs every variant (F3–F6 and the volume keys
// are OGU-only) — this is a shared vocabulary, not per-profile surface.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    /// Physical "A" button (right-side, confirm).
    A,
    /// Physical "B" button (bottom, cancel).
    B,
    /// Physical "X" button.
    X,
    /// Physical "Y" button.
    Y,
    /// Left shoulder.
    L1,
    /// Right shoulder.
    R1,
    /// Left trigger.
    L2,
    /// Right trigger.
    R2,
    /// L3 (left stick click) — full-size pads only.
    L3,
    /// R3 (right stick click) — full-size pads only.
    R3,
    Start,
    Select,
    /// Center / "Home" / Guide button (8BitDo, Xbox, PS).
    Home,
    /// OGU-only auxiliary keys (F3–F6 silkscreened buttons).
    F3,
    F4,
    F5,
    F6,
    VolumeUp,
    VolumeDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy)]
pub struct ButtonEvent {
    pub button: Button,
    pub state: ButtonState,
}
