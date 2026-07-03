//! Controller-driven on-screen keyboard.
//!
//! Renders as a wide popover anchored bottom-center over whatever the
//! current foreground is (browser, dashboard, etc.). The user drives it
//! with the gamepad: D-pad picks a key, A types it, B closes, Y toggles
//! shift, Select toggles symbol layer. Typing reaches the focused
//! Wayland client via wtype.

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, Label, Orientation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Lower,
    Upper,
    Symbol,
}

#[derive(Clone, Copy, Debug)]
pub enum KeyAction {
    Char(char),
    Space,
    Backspace,
    Enter,
    ToggleShift,
    ToggleSymbols,
    Close,
}

/// Layout rows; each row is a flat list of `KeyAction`s. The keys roughly
/// match a PSP/PS Vita on-screen keyboard's footprint — ~10 per main row
/// plus a special-keys row at the bottom.
pub struct Layout {
    pub rows: Vec<Vec<KeyAction>>,
}

pub fn layout_for(layer: Layer) -> Layout {
    fn c(s: &str) -> Vec<KeyAction> {
        s.chars().map(KeyAction::Char).collect()
    }
    let rows = match layer {
        Layer::Lower => vec![
            c("1234567890"),
            c("qwertyuiop"),
            c("asdfghjkl"),
            c("zxcvbnm,.?"),
            vec![
                KeyAction::ToggleShift,
                KeyAction::ToggleSymbols,
                KeyAction::Space,
                KeyAction::Backspace,
                KeyAction::Enter,
                KeyAction::Close,
            ],
        ],
        Layer::Upper => vec![
            c("1234567890"),
            c("QWERTYUIOP"),
            c("ASDFGHJKL"),
            c("ZXCVBNM,.?"),
            vec![
                KeyAction::ToggleShift,
                KeyAction::ToggleSymbols,
                KeyAction::Space,
                KeyAction::Backspace,
                KeyAction::Enter,
                KeyAction::Close,
            ],
        ],
        Layer::Symbol => vec![
            c("!@#$%^&*()"),
            c("`~_-=+[]{}"),
            c("\\|;:'\"<>/"),
            c("?,.€£¥•·…"),
            vec![
                KeyAction::ToggleShift,
                KeyAction::ToggleSymbols,
                KeyAction::Space,
                KeyAction::Backspace,
                KeyAction::Enter,
                KeyAction::Close,
            ],
        ],
    };
    Layout { rows }
}

/// Build the keyboard widget for a given layer. Returns the root box plus
/// a flat list of (row, col) indexed buttons + the underlying KeyAction
/// for each one — the shell uses those to drive focus and dispatch.
pub fn build(layer: Layer) -> (GtkBox, Vec<Vec<Button>>, Vec<Vec<KeyAction>>) {
    let layout = layout_for(layer);
    let container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .halign(Align::Center)
        .build();
    container.add_css_class("osk");

    let mut buttons: Vec<Vec<Button>> = Vec::new();
    let mut actions: Vec<Vec<KeyAction>> = Vec::new();

    for row_actions in &layout.rows {
        let row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(6)
            .halign(Align::Center)
            .build();
        row.add_css_class("osk-row");

        let mut row_buttons = Vec::with_capacity(row_actions.len());
        let mut row_action_copy = Vec::with_capacity(row_actions.len());
        for action in row_actions {
            let btn = build_key(*action);
            row.append(&btn);
            row_buttons.push(btn);
            row_action_copy.push(*action);
        }
        container.append(&row);
        buttons.push(row_buttons);
        actions.push(row_action_copy);
    }

    (container, buttons, actions)
}

fn build_key(action: KeyAction) -> Button {
    let btn = Button::builder().build();
    btn.add_css_class("osk-key");
    let label_text = key_label(action);
    let wide = matches!(
        action,
        KeyAction::Space | KeyAction::Backspace | KeyAction::Enter
    );
    if wide {
        btn.add_css_class("osk-wide");
    }
    if matches!(
        action,
        KeyAction::ToggleShift | KeyAction::ToggleSymbols | KeyAction::Close
    ) {
        btn.add_css_class("osk-mod");
    }
    let label = Label::builder().label(&label_text).build();
    label.add_css_class("osk-key-label");
    btn.set_child(Some(&label));
    btn
}

fn key_label(action: KeyAction) -> String {
    match action {
        KeyAction::Char(c) => c.to_string(),
        KeyAction::Space => "Space".to_string(),
        KeyAction::Backspace => "⌫".to_string(),
        KeyAction::Enter => "Enter".to_string(),
        KeyAction::ToggleShift => "⇧ Shift".to_string(),
        KeyAction::ToggleSymbols => "#+=".to_string(),
        KeyAction::Close => "Close".to_string(),
    }
}

/// Perform the side-effect of a key press. Closes/toggles are handled in
/// the shell; here we only emit keystrokes for character / whitespace /
/// backspace / enter.
///
/// We route through `ydotool` instead of `wtype` because ydotool is already
/// installed on the HTPC image (used by the existing context-menu
/// keystrokes) and ydotool's uinput backend works for any focused client,
/// not just those that bind virtual-keyboard. See task #23 for the
/// future plan to swap both for a native zwp_virtual_keyboard_v1 binding.
pub fn emit(action: KeyAction) {
    match action {
        KeyAction::Char(c) => ydotool_type(&c.to_string()),
        KeyAction::Space => ydotool_key("57:1 57:0"),       // KEY_SPACE
        KeyAction::Backspace => ydotool_key("14:1 14:0"),   // KEY_BACKSPACE
        KeyAction::Enter => ydotool_key("28:1 28:0"),       // KEY_ENTER
        _ => {}
    }
}

fn ydotool_type(s: &str) {
    log::info!("ydotool type {:?}", s);
    let _ = std::process::Command::new("ydotool")
        .args(["type", "--", s])
        .spawn();
}

fn ydotool_key(combo: &str) {
    log::info!("ydotool key {}", combo);
    let parts: Vec<&str> = combo.split_whitespace().collect();
    let _ = std::process::Command::new("ydotool")
        .arg("key")
        .args(&parts)
        .spawn();
}
