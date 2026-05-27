use crate::button::{Button, ButtonState};
use crate::joystick_cursor::JoystickCursor;
use crate::keyboard_grid::KeyboardGrid;
use crate::state::NavMode;
use crate::{niri, wtype, ydotool};
use anyhow::Result;
use log::info;

/// Result of dispatching a button event.
pub enum ActionResult {
    /// Stay in current mode.
    Stay,
    /// Transition to a new mode.
    Transition(NavMode),
}

/// Modifier state tracked by the main loop.
#[derive(Debug, Default)]
pub struct Modifiers {
    pub l1: bool,
    pub r1: bool,
    pub f3: bool,
}

/// Dispatch a button event based on current mode and modifiers.
pub async fn dispatch(
    mode: NavMode,
    button: Button,
    state: ButtonState,
    mods: &Modifiers,
    grid: &mut KeyboardGrid,
    cursor: &mut JoystickCursor,
) -> Result<ActionResult> {
    match mode {
        NavMode::WindowNav => dispatch_window_nav(button, state, mods, cursor).await,
        NavMode::ElementNav => dispatch_element_nav(button, state, mods, grid, cursor).await,
        NavMode::TextEntry => dispatch_text_entry(button, state, mods, grid).await,
        NavMode::Launcher => dispatch_launcher(button, state, mods, cursor).await,
        NavMode::GamePassthrough => dispatch_passthrough(button, state, mods).await,
    }
}

async fn dispatch_window_nav(
    button: Button,
    state: ButtonState,
    mods: &Modifiers,
    cursor: &mut JoystickCursor,
) -> Result<ActionResult> {
    if state != ButtonState::Pressed {
        return Ok(ActionResult::Stay);
    }

    // L1+R1 held = passthrough combo pending, suppress all actions
    if mods.l1 && mods.r1 {
        return Ok(ActionResult::Stay);
    }

    // F3 modifier combos
    if mods.f3 {
        match button {
            Button::X => niri::close_window().await?,
            _ => {}
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
            // nwg-drawer (layer-shell overlay) needs ydotool which needs
            // CONFIG_INPUT_UINPUT in the kernel. Fall back to fzf launcher
            // in alacritty if ydotool isn't available.
            if std::path::Path::new("/dev/uinput").exists() {
                niri::spawn(&["nwg-drawer"]).await?;
                info!("-> LAUNCHER (nwg-drawer)");
                return Ok(ActionResult::Transition(NavMode::Launcher));
            } else {
                niri::spawn(&["alacritty-mesa", "-e", "/usr/local/bin/app-launcher.sh"]).await?;
                info!("-> LAUNCHER (fzf)");
                return Ok(ActionResult::Transition(NavMode::Launcher));
            }
        }
        _ => {}
    }

    Ok(ActionResult::Stay)
}

async fn dispatch_element_nav(
    button: Button,
    state: ButtonState,
    mods: &Modifiers,
    grid: &mut KeyboardGrid,
    cursor: &mut JoystickCursor,
) -> Result<ActionResult> {
    if state != ButtonState::Pressed {
        return Ok(ActionResult::Stay);
    }

    // L1+R1 held = passthrough combo pending, suppress all actions
    if mods.l1 && mods.r1 {
        return Ok(ActionResult::Stay);
    }

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
        _ => {}
    }

    Ok(ActionResult::Stay)
}

async fn dispatch_text_entry(
    button: Button,
    state: ButtonState,
    mods: &Modifiers,
    grid: &mut KeyboardGrid,
) -> Result<ActionResult> {
    if state != ButtonState::Pressed {
        return Ok(ActionResult::Stay);
    }

    // L1+R1 held = passthrough combo pending, suppress all actions
    if mods.l1 && mods.r1 {
        return Ok(ActionResult::Stay);
    }

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

async fn dispatch_launcher(
    button: Button,
    state: ButtonState,
    mods: &Modifiers,
    cursor: &mut JoystickCursor,
) -> Result<ActionResult> {
    if state != ButtonState::Pressed {
        return Ok(ActionResult::Stay);
    }

    // L1+R1 held = passthrough combo pending, suppress all actions
    if mods.l1 && mods.r1 {
        return Ok(ActionResult::Stay);
    }

    // Use ydotool (uinput) for nwg-drawer overlay, wtype for fzf terminal
    let use_ydotool = std::path::Path::new("/dev/uinput").exists();

    match button {
        Button::DpadUp => {
            if use_ydotool { ydotool::up().await? } else { wtype::up().await? }
        }
        Button::DpadDown => {
            if use_ydotool { ydotool::down().await? } else { wtype::down().await? }
        }
        Button::DpadLeft => {
            if use_ydotool { ydotool::left().await? } else { wtype::left().await? }
        }
        Button::DpadRight => {
            if use_ydotool { ydotool::right().await? } else { wtype::right().await? }
        }
        Button::A => {
            if use_ydotool { ydotool::enter().await? } else { wtype::enter().await? }
        }
        Button::R2 => cursor.click_left(),
        Button::L2 => cursor.click_right(),
        Button::B => {
            if use_ydotool { ydotool::escape().await? } else { wtype::escape().await? }
            info!("-> WINDOW_NAV (launcher dismissed)");
            return Ok(ActionResult::Transition(NavMode::WindowNav));
        }
        Button::Start => {
            // Home toggles launcher off
            if use_ydotool { ydotool::escape().await? } else { wtype::escape().await? }
            info!("-> WINDOW_NAV (Home toggle)");
            return Ok(ActionResult::Transition(NavMode::WindowNav));
        }
        _ => {}
    }

    Ok(ActionResult::Stay)
}

async fn dispatch_passthrough(
    _button: Button,
    _state: ButtonState,
    _mods: &Modifiers,
) -> Result<ActionResult> {
    // In passthrough, we don't intercept anything.
    // The exit combo (L1+R1+Start hold) is detected in main.rs.
    Ok(ActionResult::Stay)
}
