//! Per-mode button dispatch for the OGU profile.

use super::joystick_cursor::JoystickCursor;
use super::keyboard_grid::KeyboardGrid;
use super::state::NavMode;
use super::{niri, wtype};
use crate::button::{Button, ButtonState};
use crate::modifiers::Modifiers;
use anyhow::Result;
use cs_proto::ShellCommand;
use log::info;

/// Result of dispatching a button event.
pub enum ActionResult {
    /// Stay in current mode.
    Stay,
    /// Transition to a new mode.
    Transition(NavMode),
}

/// Dispatch a button event based on current mode and modifiers. `f3` is
/// the OGU-only modifier tracked by the profile.
pub async fn dispatch(
    mode: NavMode,
    button: Button,
    state: ButtonState,
    mods: &Modifiers,
    f3: bool,
    grid: &mut KeyboardGrid,
    cursor: &mut JoystickCursor,
) -> Result<ActionResult> {
    if state != ButtonState::Pressed {
        return Ok(ActionResult::Stay);
    }

    // L1+R1 held = passthrough combo pending, suppress all actions.
    if mods.l1 && mods.r1 {
        return Ok(ActionResult::Stay);
    }

    match mode {
        NavMode::WindowNav => dispatch_window_nav(button, mods, f3, cursor).await,
        NavMode::ElementNav => dispatch_element_nav(button, mods, grid, cursor).await,
        NavMode::TextEntry => dispatch_text_entry(button, grid).await,
        NavMode::Shell => dispatch_shell(button),
        // Ungrabbed modes never reach dispatch (should_dispatch = false).
        NavMode::GamePassthrough | NavMode::Emulation => Ok(ActionResult::Stay),
    }
}

async fn dispatch_window_nav(
    button: Button,
    mods: &Modifiers,
    f3: bool,
    cursor: &mut JoystickCursor,
) -> Result<ActionResult> {
    // F3 modifier combos
    if f3 {
        if button == Button::X {
            niri::close_window().await?;
        }
        return Ok(ActionResult::Stay);
    }

    // L1 modifier combos
    if mods.l1 {
        match button {
            Button::DpadLeft => niri::move_column_left().await?,
            Button::DpadRight => niri::move_column_right().await?,
            Button::DpadUp => niri::move_window_to_workspace_up().await?,
            Button::DpadDown => niri::move_window_to_workspace_down().await?,
            Button::A => niri::close_window().await?,
            Button::Y => niri::fullscreen_window().await?,
            _ => {}
        }
        return Ok(ActionResult::Stay);
    }

    // R1 modifier combos (consume/expel)
    if mods.r1 {
        match button {
            Button::DpadLeft => niri::consume_window_into_column().await?,
            Button::DpadRight => niri::expel_window_from_column().await?,
            _ => {}
        }
        return Ok(ActionResult::Stay);
    }

    // Unmodified
    match button {
        Button::DpadLeft => niri::focus_column_left().await?,
        Button::DpadRight => niri::focus_column_right().await?,
        Button::DpadUp => niri::focus_workspace_up().await?,
        Button::DpadDown => niri::focus_workspace_down().await?,
        Button::A => {
            info!("-> ELEMENT_NAV");
            return Ok(ActionResult::Transition(NavMode::ElementNav));
        }
        Button::R2 => cursor.click_left(),
        Button::L2 => cursor.click_right(),
        Button::Start => {
            // Home button: summon the controller-shell overlay (mode
            // tiles + app launcher). Show is sent by on_transition.
            info!("-> SHELL");
            return Ok(ActionResult::Transition(NavMode::Shell));
        }
        _ => {}
    }

    Ok(ActionResult::Stay)
}

async fn dispatch_element_nav(
    button: Button,
    mods: &Modifiers,
    grid: &mut KeyboardGrid,
    cursor: &mut JoystickCursor,
) -> Result<ActionResult> {
    // L1+X -> text entry (spawn overlay)
    if mods.l1 && button == Button::X {
        info!("-> TEXT_ENTRY");
        grid.spawn_overlay()?;
        return Ok(ActionResult::Transition(NavMode::TextEntry));
    }

    // L1 modifier combos (extended navigation)
    if mods.l1 {
        match button {
            Button::DpadUp => wtype::page_up().await?,
            Button::DpadDown => wtype::page_down().await?,
            Button::DpadLeft => wtype::home().await?,
            Button::DpadRight => wtype::end().await?,
            _ => {}
        }
        return Ok(ActionResult::Stay);
    }

    // Unmodified
    match button {
        Button::DpadUp => wtype::shift_tab().await?,
        Button::DpadDown => wtype::tab().await?,
        Button::DpadLeft => wtype::left().await?,
        Button::DpadRight => wtype::right().await?,
        Button::A => wtype::enter().await?,
        Button::Y => wtype::escape().await?,
        Button::R2 => cursor.click_left(),
        Button::L2 => cursor.click_right(),
        Button::B => {
            info!("-> WINDOW_NAV");
            return Ok(ActionResult::Transition(NavMode::WindowNav));
        }
        Button::Start => {
            info!("-> SHELL");
            return Ok(ActionResult::Transition(NavMode::Shell));
        }
        _ => {}
    }

    Ok(ActionResult::Stay)
}

async fn dispatch_text_entry(button: Button, grid: &mut KeyboardGrid) -> Result<ActionResult> {
    match button {
        // D-pad navigates the keyboard grid
        Button::DpadUp => {
            grid.move_up();
            grid.send_position();
        }
        Button::DpadDown => {
            grid.move_down();
            grid.send_position();
        }
        Button::DpadLeft => {
            grid.move_left();
            grid.send_position();
        }
        Button::DpadRight => {
            grid.move_right();
            grid.send_position();
        }
        // A = type the highlighted character
        Button::A => {
            let ch = grid.current_char();
            wtype::type_text(&ch).await?;
        }
        // X = space
        Button::X => wtype::space().await?,
        // Y = backspace
        Button::Y => wtype::backspace().await?,
        // Select = toggle shift
        Button::Select => {
            grid.toggle_shift();
            grid.send_position();
        }
        // L1 = cursor left in text field
        Button::L1 => wtype::left().await?,
        // R1 = cursor right in text field
        Button::R1 => wtype::right().await?,
        // B = dismiss overlay, return to ElementNav
        Button::B => {
            info!("-> ELEMENT_NAV (hiding OSK overlay)");
            grid.kill_overlay();
            grid.reset();
            return Ok(ActionResult::Transition(NavMode::ElementNav));
        }
        _ => {}
    }

    Ok(ActionResult::Stay)
}

/// Shell mode: the controller-shell overlay is up; forward intent-level
/// commands over its socket. The shell owns the interaction (views,
/// context menu, back-stack) — when it dismisses itself it sends
/// `Mode{window_nav}` back on our socket, which is what pops this mode.
fn dispatch_shell(button: Button) -> Result<ActionResult> {
    let cmd = match button {
        Button::DpadUp => Some(ShellCommand::Up),
        Button::DpadDown => Some(ShellCommand::Down),
        Button::DpadLeft => Some(ShellCommand::Left),
        Button::DpadRight => Some(ShellCommand::Right),
        Button::A => Some(ShellCommand::Select),
        Button::B => Some(ShellCommand::Back),
        Button::Y => Some(ShellCommand::Options),
        // Shoulder buttons hop between the shell's views.
        Button::L1 => Some(ShellCommand::View {
            name: "modes".into(),
        }),
        Button::R1 => Some(ShellCommand::View {
            name: "apps".into(),
        }),
        _ => None,
    };
    if let Some(c) = cmd {
        cs_proto::sock::send_shell(&c);
        return Ok(ActionResult::Stay);
    }
    // Home toggles the shell back off — a local escape hatch that works
    // even if the shell is down.
    if button == Button::Start {
        info!("-> WINDOW_NAV (Home toggle)");
        return Ok(ActionResult::Transition(NavMode::WindowNav));
    }
    Ok(ActionResult::Stay)
}
