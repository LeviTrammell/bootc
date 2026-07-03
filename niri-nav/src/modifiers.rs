//! Shared modifier state across all profiles.

use crate::button::Button;

#[derive(Debug, Default, Clone, Copy)]
pub struct Modifiers {
    pub l1: bool,
    pub r1: bool,
}

impl Modifiers {
    pub fn update(&mut self, button: Button, pressed: bool) {
        match button {
            Button::L1 => self.l1 = pressed,
            Button::R1 => self.r1 = pressed,
            _ => {}
        }
    }
}
