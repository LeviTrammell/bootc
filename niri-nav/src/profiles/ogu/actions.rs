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
/// the OGU-only modifier tracked by the profile. `text_entry_return` is
/// where dismissing the OSK goes back to — set on TextEntry entry so the
/// browser and element-nav flows each return to their own mode.
pub async fn dispatch(
    mode: NavMode,
    button: Button,
    state: ButtonState,
    mods: &Modifiers,
    f3: bool,
    grid: &mut KeyboardGrid,
    cursor: &mut JoystickCursor,
    text_entry_return: &mut NavMode,
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
        NavMode::ElementNav => {
            dispatch_element_nav(button, mods, cursor, text_entry_return).await
        }
        NavMode::TextEntry => dispatch_text_entry(button, grid, *text_entry_return).await,
        NavMode::Shell => dispatch_shell(button),
        NavMode::Browser => {
            dispatch_browser(button, mods, f3, cursor, text_entry_return).await
        }
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
    cursor: &mut JoystickCursor,
    text_entry_return: &mut NavMode,
) -> Result<ActionResult> {
    // L1+X -> text entry (overlay shown by on_transition)
    if mods.l1 && button == Button::X {
        info!("-> TEXT_ENTRY");
        *text_entry_return = NavMode::ElementNav;
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

async fn dispatch_text_entry(
    button: Button,
    grid: &mut KeyboardGrid,
    return_mode: NavMode,
) -> Result<ActionResult> {
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
        // Select = cycle layer (lowercase -> UPPER -> symbols)
        Button::Select => {
            grid.cycle_layer();
            grid.send_position();
        }
        // L1 = cursor left in text field
        Button::L1 => wtype::left().await?,
        // R1 = cursor right in text field
        Button::R1 => wtype::right().await?,
        // Start = Return (submit URL / form)
        Button::Start => wtype::enter().await?,
        // B = dismiss overlay, return to wherever we came from
        // (on_transition hides the overlay and resets the grid)
        Button::B => {
            info!("-> {} (hiding OSK overlay)", return_mode);
            return Ok(ActionResult::Transition(return_mode));
        }
        _ => {}
    }

    Ok(ActionResult::Stay)
}

/// Browser mode (Zen): PSP-era browsing on a handheld. The left stick
/// (via joystick-cursor) is the mouse, the right stick scrolls; here we
/// map the digital inputs:
///   D-pad Down/Up = next/prev interactable (Tab / Shift+Tab —
///     landing on a text field auto-summons the OSK via input-method)
///   A = activate focused element (Enter)    X = click at cursor
///   R2 = left click    L2 = right click     B = back
///   Y = OSK    Select = URL bar + OSK    L1/R1 = prev/next tab
///   F3+A = new tab  F3+X = close tab  F3+B = forward  F3+Y = reload
///   D-pad Left/Right = arrow keys   Start = Shell
async fn dispatch_browser(
    button: Button,
    _mods: &Modifiers,
    f3: bool,
    cursor: &mut JoystickCursor,
    text_entry_return: &mut NavMode,
) -> Result<ActionResult> {
    if f3 {
        match button {
            Button::A => wtype::new_tab().await?,
            Button::X => wtype::close_tab().await?,
            Button::B => wtype::browser_forward().await?,
            Button::Y => wtype::reload_page().await?,
            _ => {}
        }
        return Ok(ActionResult::Stay);
    }

    match button {
        Button::A => wtype::enter().await?,
        Button::X => cursor.click_left(),
        Button::R2 => cursor.click_left(),
        Button::L2 => cursor.click_right(),
        Button::B => wtype::browser_back().await?,
        Button::L1 => wtype::prev_tab().await?,
        Button::R1 => wtype::next_tab().await?,
        Button::DpadUp => wtype::shift_tab().await?,
        Button::DpadDown => wtype::tab().await?,
        Button::DpadLeft => wtype::left().await?,
        Button::DpadRight => wtype::right().await?,
        // Y = OSK for the focused field
        Button::Y => {
            info!("-> TEXT_ENTRY (from browser)");
            *text_entry_return = NavMode::Browser;
            return Ok(ActionResult::Transition(NavMode::TextEntry));
        }
        // Select = jump to the URL bar; the resulting text-field focus
        // also auto-summons the OSK via input-method-v2, but we
        // transition immediately so the d-pad drives the grid.
        Button::Select => {
            info!("-> TEXT_ENTRY (URL bar)");
            wtype::focus_urlbar().await?;
            *text_entry_return = NavMode::Browser;
            return Ok(ActionResult::Transition(NavMode::TextEntry));
        }
        Button::Start => {
            info!("-> SHELL");
            return Ok(ActionResult::Transition(NavMode::Shell));
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
