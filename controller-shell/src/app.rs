//! gtk4 application glue. Two views for now (modes + apps); a `View` enum
//! tracks which is active, focus is per-view, navigation commands operate
//! on the active view.

use crate::config::{ShellConfig, Tile};
use crate::desktop::{self, AppEntry};
use crate::ipc::{self, Command};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, FlowBox, Image,
    Label, Orientation, Popover, PositionType, ScrolledWindow,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::io::{self, BufRead};
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;

const CSS: &str = include_str!("style.css");

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Modes,
    Apps,
}

/// What's running in the foreground. The shell uses this to pick the
/// right context menu when Y/Triangle is pressed. In production this
/// state is fed from niri-nav (which knows the current mode); for dev
/// we honour `CONTROLLER_SHELL_FOREGROUND` to fake it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Foreground {
    Shell,
    Browser,
}

#[derive(Clone, Copy, Debug)]
enum ContextAction {
    // App-drawer (Apps view)
    Launch,
    // Browser (Dashboard mode — chromium kiosk)
    BrowserRefresh,
    BrowserBack,
    BrowserForward,
    BrowserHome,
    BrowserZoomIn,
    BrowserZoomOut,
    BrowserFullscreen,
    BrowserSwitchDashboard,
    BrowserExit,
}

impl ContextAction {
    fn label(self) -> &'static str {
        match self {
            ContextAction::Launch => "Launch",
            ContextAction::BrowserRefresh => "Refresh page",
            ContextAction::BrowserBack => "Back",
            ContextAction::BrowserForward => "Forward",
            ContextAction::BrowserHome => "Dashboard home",
            ContextAction::BrowserZoomIn => "Zoom in",
            ContextAction::BrowserZoomOut => "Zoom out",
            ContextAction::BrowserFullscreen => "Toggle fullscreen",
            ContextAction::BrowserSwitchDashboard => "Switch dashboard…",
            ContextAction::BrowserExit => "Exit to mode picker",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            ContextAction::Launch => "media-playback-start-symbolic",
            ContextAction::BrowserRefresh => "view-refresh-symbolic",
            ContextAction::BrowserBack => "go-previous-symbolic",
            ContextAction::BrowserForward => "go-next-symbolic",
            ContextAction::BrowserHome => "go-home-symbolic",
            ContextAction::BrowserZoomIn => "zoom-in-symbolic",
            ContextAction::BrowserZoomOut => "zoom-out-symbolic",
            ContextAction::BrowserFullscreen => "view-fullscreen-symbolic",
            ContextAction::BrowserSwitchDashboard => "web-browser-symbolic",
            ContextAction::BrowserExit => "application-exit-symbolic",
        }
    }
}

struct ContextMenu {
    popover: Popover,
    buttons: Vec<Button>,
    actions: Vec<ContextAction>,
    focus: usize,
}

struct Osk {
    container: GtkBox,
    rows: Vec<Vec<Button>>,
    actions: Vec<Vec<crate::osk::KeyAction>>,
    row: usize,
    col: usize,
    layer: crate::osk::Layer,
    shift_latched: bool,
}

impl Osk {
    fn update_focus(&self) {
        // Highlight only — do NOT call grab_focus. We need the focused
        // Wayland client below us (browser URL bar, terminal, etc.) to
        // keep keyboard focus so wtype's keystrokes reach it. Gamepad
        // navigation arrives via stdin so we don't need gtk focus to
        // drive the OSK.
        for (r, row) in self.rows.iter().enumerate() {
            for (c, btn) in row.iter().enumerate() {
                if r == self.row && c == self.col {
                    btn.add_css_class("focused");
                } else {
                    btn.remove_css_class("focused");
                }
            }
        }
    }
}

struct ShellState {
    window: ApplicationWindow,
    stack: gtk4::Stack,
    config: ShellConfig,
    view: View,
    // Modes view
    mode_tiles: Vec<Button>,
    mode_focus: usize,
    // Apps view
    app_entries: Vec<AppEntry>,
    app_tiles: Vec<Button>,
    app_focus: usize,
    apps_per_row: usize,
    visible: bool,
    context: Option<ContextMenu>,
    osk: Option<Osk>,
    foreground: Foreground,
}

impl ShellState {
    fn current_focus(&self) -> usize {
        match self.view {
            View::Modes => self.mode_focus,
            View::Apps => self.app_focus,
        }
    }

    fn focus_count(&self) -> usize {
        match self.view {
            View::Modes => self.mode_tiles.len(),
            View::Apps => self.app_tiles.len(),
        }
    }

    fn set_focus(&mut self, idx: usize) {
        let count = self.focus_count();
        if count == 0 {
            return;
        }
        let idx = idx.min(count - 1);
        let tiles: &[Button] = match self.view {
            View::Modes => &self.mode_tiles,
            View::Apps => &self.app_tiles,
        };
        for (i, tile) in tiles.iter().enumerate() {
            if i == idx {
                tile.add_css_class("focused");
                tile.grab_focus();
            } else {
                tile.remove_css_class("focused");
            }
        }
        match self.view {
            View::Modes => self.mode_focus = idx,
            View::Apps => self.app_focus = idx,
        }
    }

    fn move_linear(&mut self, delta: isize) {
        let len = self.focus_count() as isize;
        if len == 0 {
            return;
        }
        let cur = self.current_focus() as isize;
        let next = (cur + delta).rem_euclid(len) as usize;
        self.set_focus(next);
    }

    fn move_grid(&mut self, dx: isize, dy: isize) {
        match self.view {
            View::Modes => {
                // Modes are a single row — only x movement.
                if dx != 0 {
                    self.move_linear(dx);
                }
            }
            View::Apps => {
                let cols = self.apps_per_row as isize;
                let len = self.app_tiles.len() as isize;
                if cols == 0 || len == 0 {
                    return;
                }
                let cur = self.app_focus as isize;
                let mut x = cur % cols;
                let mut y = cur / cols;
                let rows = (len + cols - 1) / cols;
                x = (x + dx).rem_euclid(cols);
                y = (y + dy).rem_euclid(rows);
                let mut next = y * cols + x;
                if next >= len {
                    next = len - 1;
                }
                self.set_focus(next as usize);
            }
        }
    }

    fn select(&mut self) {
        if let Some(ctx) = &self.context {
            let action = ctx.actions[ctx.focus];
            self.invoke_context_action(action);
            self.close_context();
            return;
        }
        match self.view {
            View::Modes => {
                let Some(tile) = self.config.tiles.get(self.mode_focus).cloned() else {
                    return;
                };
                log::info!("select tile={}", tile.key);
                // A view tile (e.g. "Apps") navigates within the shell
                // instead of dismissing it.
                if let Some(view) = tile.view.as_deref() {
                    if let Some(v) = view_from_name(view) {
                        self.switch_view(v);
                    }
                    return;
                }
                if let Some(exec) = &tile.exec {
                    if !exec.is_empty() {
                        let _ = std::process::Command::new(&exec[0])
                            .args(&exec[1..])
                            .spawn();
                    }
                }
                // Tell the nav adapter: it will grab/ungrab, manage the
                // cursor daemon, chvt, etc. as its profile dictates.
                if let Some(nm) = &tile.nav_mode {
                    send_niri_nav_mode(nm);
                }
                // Self-hide; the nav adapter will also send Hide on its
                // own transition, but doing it here avoids a visible
                // flicker.
                self.hide();
            }
            View::Apps => {
                if let Some(app) = self.app_entries.get(self.app_focus) {
                    desktop::launch(app, &self.config);
                    // App launched — pop ourselves out of the way and
                    // tell the nav adapter to enter its app-foreground
                    // mode (Desktop on HTPC, WindowNav on OGU).
                    send_niri_nav_mode(&self.config.app_launch_nav_mode);
                    self.hide();
                }
            }
        }
    }

    /// Back from the root: Apps view pops to the modes row; the modes
    /// row dismisses the shell and, if configured, hands control back
    /// to the nav adapter's exit mode.
    fn back(&mut self) {
        match self.view {
            View::Apps => self.switch_view(View::Modes),
            View::Modes => {
                if let Some(nm) = &self.config.exit_nav_mode {
                    send_niri_nav_mode(nm);
                }
                self.hide();
            }
        }
    }

    fn open_context(&mut self) {
        if self.context.is_some() {
            return;
        }
        let (parent, actions, anchor) = match self.foreground {
            Foreground::Browser => {
                // Anchor to the dedicated invisible bottom-center anchor.
                let root = match self.window.child() {
                    Some(c) => c,
                    None => return,
                };
                let anchor = find_named_descendant(&root, "browser-popover-anchor")
                    .unwrap_or(root);
                (
                    anchor,
                    vec![
                        ContextAction::BrowserRefresh,
                        ContextAction::BrowserBack,
                        ContextAction::BrowserForward,
                        ContextAction::BrowserHome,
                        ContextAction::BrowserZoomIn,
                        ContextAction::BrowserZoomOut,
                        ContextAction::BrowserFullscreen,
                        ContextAction::BrowserSwitchDashboard,
                        ContextAction::BrowserExit,
                    ],
                    PositionType::Top,
                )
            }
            Foreground::Shell => {
                if self.view != View::Apps {
                    return;
                }
                let parent: gtk4::Widget = match self.app_tiles.get(self.app_focus) {
                    Some(t) => t.clone().upcast(),
                    None => return,
                };
                (parent, vec![ContextAction::Launch], PositionType::Right)
            }
        };
        let (popover, buttons) = build_context_popover(&parent, &actions, anchor);
        // Focus the first item.
        if let Some(btn) = buttons.first() {
            btn.add_css_class("focused");
            btn.grab_focus();
        }
        popover.popup();
        self.context = Some(ContextMenu {
            popover,
            buttons,
            actions,
            focus: 0,
        });
    }

    fn close_context(&mut self) {
        if let Some(ctx) = self.context.take() {
            ctx.popover.popdown();
        }
    }

    fn open_osk(&mut self) {
        log::info!("open_osk()");
        if self.osk.is_some() {
            return;
        }
        // The OSK is summonable from any mode (Dashboard, Picker, ...)
        // so the shell window may currently be hidden. Always force it
        // visible while OSK is up.
        if !self.visible {
            self.show();
        }
        let root = match self.window.child() {
            Some(c) => c,
            None => return,
        };
        let root_box = match root.downcast::<GtkBox>() {
            Ok(b) => b,
            Err(_) => {
                log::warn!("root is not a GtkBox");
                return;
            }
        };
        let layer = crate::osk::Layer::Lower;
        let (widget, rows, actions) = crate::osk::build(layer);
        // Wrap in an outer container so the OSK floats at the bottom of
        // the screen rather than expanding to fill the root box.
        let container = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .halign(Align::Center)
            .valign(Align::End)
            .build();
        container.add_css_class("osk-floating");
        container.append(&widget);
        root_box.append(&container);

        let osk = Osk {
            container,
            rows,
            actions,
            row: 1,
            col: 0,
            layer,
            shift_latched: false,
        };
        osk.update_focus();
        self.osk = Some(osk);
    }

    fn close_osk(&mut self) {
        if let Some(osk) = self.osk.take() {
            if let Some(parent) = osk.container.parent() {
                if let Ok(parent_box) = parent.downcast::<GtkBox>() {
                    parent_box.remove(&osk.container);
                }
            }
        }
    }

    fn osk_move(&mut self, dx: isize, dy: isize) {
        let Some(osk) = self.osk.as_mut() else {
            return;
        };
        if osk.rows.is_empty() {
            return;
        }
        let rows_count = osk.rows.len() as isize;
        let mut row = osk.row as isize;
        let mut col = osk.col as isize;
        row = (row + dy).rem_euclid(rows_count);
        let cols_in_row = osk.rows[row as usize].len() as isize;
        col = (col + dx).rem_euclid(cols_in_row.max(1));
        if col >= cols_in_row {
            col = cols_in_row - 1;
        }
        osk.row = row as usize;
        osk.col = col as usize;
        osk.update_focus();
    }

    fn osk_select(&mut self) {
        let action = {
            let Some(osk) = self.osk.as_ref() else {
                return;
            };
            osk.actions[osk.row][osk.col]
        };
        use crate::osk::KeyAction;
        match action {
            KeyAction::ToggleShift => {
                if let Some(osk) = self.osk.as_mut() {
                    osk.shift_latched = !osk.shift_latched;
                    let next_layer = if osk.shift_latched {
                        crate::osk::Layer::Upper
                    } else {
                        crate::osk::Layer::Lower
                    };
                    self.rebuild_osk(next_layer);
                }
            }
            KeyAction::ToggleSymbols => {
                if let Some(osk) = self.osk.as_ref() {
                    let next_layer = match osk.layer {
                        crate::osk::Layer::Symbol => {
                            if osk.shift_latched {
                                crate::osk::Layer::Upper
                            } else {
                                crate::osk::Layer::Lower
                            }
                        }
                        _ => crate::osk::Layer::Symbol,
                    };
                    self.rebuild_osk(next_layer);
                }
            }
            KeyAction::Close => self.close_osk(),
            other => {
                crate::osk::emit(other);
                // After typing a character, drop a latched shift like a
                // soft keyboard: shift only affects the next key.
                if let Some(osk) = self.osk.as_mut() {
                    if osk.shift_latched && matches!(other, KeyAction::Char(_)) {
                        osk.shift_latched = false;
                        self.rebuild_osk(crate::osk::Layer::Lower);
                    }
                }
            }
        }
    }

    fn rebuild_osk(&mut self, layer: crate::osk::Layer) {
        let Some(osk) = self.osk.as_mut() else {
            return;
        };
        // Remove old keyboard widget and add the new one.
        while let Some(child) = osk.container.first_child() {
            osk.container.remove(&child);
        }
        let (widget, rows, actions) = crate::osk::build(layer);
        osk.container.append(&widget);
        osk.rows = rows;
        osk.actions = actions;
        osk.layer = layer;
        if osk.row >= osk.rows.len() {
            osk.row = osk.rows.len().saturating_sub(1);
        }
        let max_col = osk.rows[osk.row].len().saturating_sub(1);
        if osk.col > max_col {
            osk.col = max_col;
        }
        osk.update_focus();
    }

    fn context_move(&mut self, delta: isize) {
        let Some(ctx) = self.context.as_mut() else {
            return;
        };
        let len = ctx.buttons.len() as isize;
        if len == 0 {
            return;
        }
        let cur = ctx.focus as isize;
        let next = (cur + delta).rem_euclid(len) as usize;
        for (i, btn) in ctx.buttons.iter().enumerate() {
            if i == next {
                btn.add_css_class("focused");
                btn.grab_focus();
            } else {
                btn.remove_css_class("focused");
            }
        }
        ctx.focus = next;
    }

    fn invoke_context_action(&self, action: ContextAction) {
        match action {
            // App-drawer actions need a focused app.
            ContextAction::Launch => {
                if let Some(app) = self.app_entries.get(self.app_focus) {
                    desktop::launch(app, &self.config);
                }
            }
            // Browser actions shell out — ydotool for keystrokes, htpc-ctl
            // for mode change. ydotool needs /dev/uinput, which the HTPC
            // bootc image already arranges.
            ContextAction::BrowserRefresh => ydotool_key("ctrl+r"),
            ContextAction::BrowserBack => ydotool_key("alt+Left"),
            ContextAction::BrowserForward => ydotool_key("alt+Right"),
            ContextAction::BrowserHome => {
                let _ = std::process::Command::new("htpc-ctl")
                    .args(["kiosk-reload"])
                    .spawn();
            }
            ContextAction::BrowserZoomIn => ydotool_key("ctrl+equal"),
            ContextAction::BrowserZoomOut => ydotool_key("ctrl+minus"),
            ContextAction::BrowserFullscreen => ydotool_key("F11"),
            ContextAction::BrowserSwitchDashboard => {
                log::info!("dashboard picker (placeholder)");
            }
            ContextAction::BrowserExit => {
                let _ = std::process::Command::new("htpc-ctl")
                    .args(["mode", "desktop"])
                    .spawn();
            }
        }
    }

    fn show(&mut self) {
        self.window.set_visible(true);
        self.visible = true;
    }

    fn hide(&mut self) {
        self.window.set_visible(false);
        self.visible = false;
    }

    fn switch_view(&mut self, view: View) {
        if self.view == view {
            return;
        }
        self.view = view;
        let name = match view {
            View::Modes => "modes",
            View::Apps => "apps",
        };
        self.stack.set_visible_child_name(name);
        self.set_focus(self.current_focus());
    }
}

pub fn on_activate(app: &Application) {
    let provider = CssProvider::new();
    provider.load_from_string(CSS);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no gdk display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let config = crate::config::load();

    let window = ApplicationWindow::builder()
        .application(app)
        .title("controller-shell")
        .build();

    let initial_view = match std::env::var("CONTROLLER_SHELL_VIEW").as_deref() {
        Ok("apps") => View::Apps,
        _ => View::Modes,
    };
    let foreground = match std::env::var("CONTROLLER_SHELL_FOREGROUND").as_deref() {
        Ok("browser") => Foreground::Browser,
        _ => Foreground::Shell,
    };

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    // In browser mode, allow input to pass through to the layer below
    // (the chromium kiosk / Zen / whatever is running) when the shell
    // hasn't captured focus. KeyboardMode::OnDemand toggles based on
    // whether a widget asks for focus — exactly what we want when the
    // popover opens.
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_namespace(Some("controller-shell"));
    if foreground == Foreground::Browser {
        // Don't reserve any screen area or block clicks behind the
        // surface when we have no popover up.
        window.add_css_class("transparent");
    }

    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(24)
        .halign(Align::Fill)
        .valign(Align::Fill)
        .hexpand(true)
        .vexpand(true)
        .build();
    root.add_css_class("root");
    // Query the connected output's geometry rather than hardcoding —
    // works on the H2+ (1280x720), the OGU (854x480) and the
    // workstation (2560x1600). gtk needs an explicit default size or it
    // requests only as much surface as the visible children, leaving the
    // picker floating in a smaller-than-screen window with black margins
    // around it.
    let (w, h) = primary_monitor_size().unwrap_or((1920, 1080));
    log::info!("window default size: {w}x{h}");
    window.set_default_size(w, h);
    // Small panels (the OGU's 854x480) get compact metrics: smaller
    // icons, tiles, and paddings via the .compact CSS scope.
    let compact = w < 1024;
    if compact {
        root.add_css_class("compact");
    }

    let header = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(16)
        .halign(Align::Center)
        .build();
    header.add_css_class("header");
    let title = Label::builder().label("Controller Shell").build();
    title.add_css_class("title");
    header.append(&title);
    root.append(&header);

    // Stack lets us swap between modes view and apps view without
    // rebuilding the layer-shell window.
    let stack = gtk4::Stack::new();
    stack.set_transition_type(gtk4::StackTransitionType::SlideUpDown);
    stack.set_transition_duration(180);
    stack.set_vexpand(true);
    stack.set_hexpand(true);

    // --- Modes view ---
    let modes_view = build_modes_view(&config.tiles, compact);
    stack.add_named(&modes_view.0, Some("modes"));

    // --- Apps view ---
    let entries = desktop::discover();
    log::info!("discovered {} apps", entries.len());
    let apps_per_row = compute_apps_per_row(w, compact);
    let apps_view = build_apps_view(&entries, apps_per_row, compact);
    stack.add_named(&apps_view.0, Some("apps"));

    root.append(&stack);

    // Bottom hint bar — explains the buttons.
    let hint = Label::builder()
        .label("A ⏵ select   ⬄ ⬍ navigate   B ⏵ back   L1/R1 ⏵ switch view   Y ⏵ options")
        .build();
    hint.add_css_class("hint-bar");
    root.append(&hint);

    window.set_child(Some(&root));

    stack.set_visible_child_name(match initial_view {
        View::Modes => "modes",
        View::Apps => "apps",
    });

    // In browser foreground, hide the shell chrome — only the OSK /
    // context popover (when opened) should be visible. We keep `root`
    // itself stretched to fill the layer-shell surface (otherwise gtk
    // collapses the surface to its visible-content size, breaking
    // popover positioning).
    if foreground == Foreground::Browser {
        header.set_visible(false);
        stack.set_visible(false);
        hint.set_visible(false);
        root.set_hexpand(true);
        root.set_vexpand(true);
        // A vexpand:true filler at the top pushes any appended floating
        // widgets (OSK, context menu) to the bottom of the layer-shell
        // surface.
        let filler = GtkBox::builder()
            .vexpand(true)
            .hexpand(true)
            .build();
        filler.set_widget_name("browser-filler");
        root.append(&filler);
        // Full-width anchor strip at the bottom; the browser context
        // popover (if we revive it) anchors here.
        let anchor = GtkBox::builder()
            .halign(Align::Fill)
            .valign(Align::End)
            .height_request(8)
            .hexpand(true)
            .build();
        anchor.set_widget_name("browser-popover-anchor");
        root.append(&anchor);
    }

    let state = Rc::new(RefCell::new(ShellState {
        window: window.clone(),
        stack,
        config,
        view: initial_view,
        mode_tiles: modes_view.1,
        mode_focus: 0,
        app_entries: entries,
        app_tiles: apps_view.1,
        app_focus: 0,
        apps_per_row,
        visible: true,
        context: None,
        osk: None,
        foreground,
    }));
    state.borrow_mut().set_focus(0);

    // Start hidden so the Dashboard (chromium kiosk / whatever) is what
    // the user sees. niri-nav sends {"cmd":"show"} when the user
    // presses HOME to summon the picker. The CONTROLLER_SHELL_START_VISIBLE
    // env lets dev runs (workstation, no niri-nav) show immediately.
    if std::env::var("CONTROLLER_SHELL_START_VISIBLE").as_deref() != Ok("1") {
        state.borrow_mut().hide();
    }

    // stdin → glib channel (dev fallback)
    let (tx, rx) = mpsc::channel::<Command>();
    let stdin_tx = tx.clone();
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            if let Some(cmd) = ipc::parse(&line) {
                if stdin_tx.send(cmd).is_err() {
                    break;
                }
            }
        }
    });

    // Unix socket → glib channel (primary path: niri-nav sends gamepad
    // nav events here).
    if let Err(e) = crate::ipc_server::spawn(tx) {
        log::warn!("ipc server: {e}");
    }

    let state_for_loop = state.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        while let Ok(cmd) = rx.try_recv() {
            handle_command(&state_for_loop, cmd);
        }
        glib::ControlFlow::Continue
    });

    // Keyboard fallback for dev (no gamepad needed).
    let key_ctl = gtk4::EventControllerKey::new();
    let state_for_keys = state.clone();
    key_ctl.connect_key_pressed(move |_, key, _, _| {
        use gtk4::gdk::Key;
        let cmd = match key {
            Key::Up | Key::w => Some(Command::Up),
            Key::Down | Key::s => Some(Command::Down),
            Key::Left | Key::a => Some(Command::Left),
            Key::Right | Key::d => Some(Command::Right),
            Key::Return | Key::space => Some(Command::Select),
            Key::Escape => Some(Command::Back),
            Key::Tab => Some(Command::View {
                name: match state_for_keys.borrow().view {
                    View::Modes => "apps".into(),
                    View::Apps => "modes".into(),
                },
            }),
            Key::y | Key::o => Some(Command::Options),
            Key::k => Some(Command::Keyboard),
            _ => None,
        };
        if let Some(c) = cmd {
            handle_command(&state_for_keys, c);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key_ctl);

    window.present();
}

fn ydotool_key(combo: &str) {
    log::info!("ydotool key {}", combo);
    let _ = std::process::Command::new("ydotool")
        .args(["key", combo])
        .spawn();
}

/// Query gdk for the first connected monitor's logical size. Returns
/// `None` if no display is available (e.g. running outside a Wayland
/// session). On a multi-monitor setup we just take monitor 0 — the
/// layer-shell will end up on whichever output the compositor picks.
fn primary_monitor_size() -> Option<(i32, i32)> {
    use gtk4::gdk;
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    let mon = monitors.item(0)?.downcast::<gdk::Monitor>().ok()?;
    let geom = mon.geometry();
    Some((geom.width(), geom.height()))
}

/// Send a mode-change command to whichever nav adapter is bound at
/// `niri-nav.sock`. The "niri-" prefix is a current convention; a
/// future hyprland-nav adapter could choose to use the same name (it's
/// the canonical "active nav" socket) or its own (e.g. `hyprland-nav.sock`).
fn send_niri_nav_mode(name: &str) {
    cs_proto::sock::send_nav(
        "niri-nav.sock",
        &cs_proto::NavCommand::Mode {
            name: name.to_string(),
        },
    );
}

/// Walk the widget tree to find a descendant by gtk widget name. Used to
/// locate the invisible popover anchor we added in browser mode.
fn find_named_descendant(root: &gtk4::Widget, name: &str) -> Option<gtk4::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(w) = child {
        if let Some(hit) = find_named_descendant(&w, name) {
            return Some(hit);
        }
        child = w.next_sibling();
    }
    None
}

/// Columns in the Apps grid, derived from the output width so the d-pad
/// math matches what's on screen. The FlowBox is pinned to exactly this
/// count (min == max children per line) — if the two ever disagreed,
/// d-pad up/down would land on the wrong tile.
fn compute_apps_per_row(width: i32, compact: bool) -> usize {
    // Effective tile width = CSS min-width + horizontal padding + border
    // + 20px column spacing (208px full / 132px compact), plus the side
    // margins (2x80px full / 2x48px compact).
    let (tile, margins) = if compact { (132, 96) } else { (208, 160) };
    (((width - margins) / tile).max(3)) as usize
}

fn handle_command(state: &Rc<RefCell<ShellState>>, cmd: Command) {
    let mut s = state.borrow_mut();
    // OSK takes precedence over everything else when open — it's modal
    // text entry, and the user explicitly opened it.
    if s.osk.is_some() {
        match cmd {
            Command::Up => s.osk_move(0, -1),
            Command::Down => s.osk_move(0, 1),
            Command::Left => s.osk_move(-1, 0),
            Command::Right => s.osk_move(1, 0),
            Command::Select => s.osk_select(),
            Command::Back | Command::Keyboard => s.close_osk(),
            Command::Hide => {
                s.close_osk();
                s.hide();
            }
            _ => {}
        }
        return;
    }
    // Context menu, when open, captures navigation + select + back.
    if s.context.is_some() {
        match cmd {
            Command::Up | Command::Left => s.context_move(-1),
            Command::Down | Command::Right => s.context_move(1),
            Command::Select => s.select(),
            Command::Back | Command::Options => s.close_context(),
            Command::Hide => {
                s.close_context();
                s.hide();
            }
            _ => {}
        }
        return;
    }
    // Browser foreground with no popover open: route nav directly to
    // keystrokes so the user can drive the browser with the pad.
    if matches!(s.foreground, Foreground::Browser) {
        match cmd {
            Command::Up => {
                ydotool_key("Page_Up");
                return;
            }
            Command::Down => {
                ydotool_key("Page_Down");
                return;
            }
            Command::Left => {
                ydotool_key("alt+Left");
                return;
            }
            Command::Right => {
                ydotool_key("alt+Right");
                return;
            }
            Command::Select => {
                ydotool_key("Return");
                return;
            }
            Command::Back => {
                ydotool_key("Escape");
                return;
            }
            Command::Options => {
                s.open_context();
                return;
            }
            Command::Keyboard => {
                s.open_osk();
                return;
            }
            Command::Show => {
                s.show();
                return;
            }
            Command::Hide => {
                s.hide();
                return;
            }
            Command::View { .. } => return,
        }
    }
    match cmd {
        Command::Show => s.show(),
        Command::Hide => s.hide(),
        Command::Up => s.move_grid(0, -1),
        Command::Down => s.move_grid(0, 1),
        Command::Left => s.move_grid(-1, 0),
        Command::Right => s.move_grid(1, 0),
        Command::Select => s.select(),
        Command::Back => s.back(),
        Command::Options => s.open_context(),
        Command::Keyboard => s.open_osk(),
        Command::View { name } => {
            if let Some(v) = view_from_name(&name) {
                s.switch_view(v);
            }
        }
    }
}

fn view_from_name(name: &str) -> Option<View> {
    match name {
        "modes" => Some(View::Modes),
        "apps" => Some(View::Apps),
        _ => None,
    }
}

fn build_modes_view(tiles_cfg: &[Tile], compact: bool) -> (GtkBox, Vec<Button>) {
    let container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .halign(Align::Fill)
        .valign(Align::Center)
        .spacing(24)
        .hexpand(true)
        .build();

    // The 7 tile row overflows a 1280-wide output. Wrap it in a horizontal
    // ScrolledWindow so the off-screen tiles are reachable; gtk's
    // ScrolledWindow auto-scrolls to keep the grab_focused widget visible.
    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::External)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .hexpand(true)
        .build();

    let row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(24)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();
    row.add_css_class("mode-row");

    let mut tiles = Vec::with_capacity(tiles_cfg.len());
    for (i, mode) in tiles_cfg.iter().enumerate() {
        let tile = build_mode_tile(mode, compact);
        // Symmetric breathing room at the row's start and end so the
        // first/last tile don't hug the screen edge after auto-scroll.
        // gtk's focus-scroll only pulls the focused widget to the
        // viewport edge — extra space has to live ON the tile.
        if i == 0 {
            tile.set_margin_start(64);
        }
        if i == tiles_cfg.len() - 1 {
            tile.set_margin_end(64);
        }
        row.append(&tile);
        tiles.push(tile);
    }
    scroller.set_child(Some(&row));
    container.append(&scroller);
    (container, tiles)
}

fn build_apps_view(entries: &[AppEntry], per_row: usize, compact: bool) -> (GtkBox, Vec<Button>) {
    let container = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .halign(Align::Fill)
        .valign(Align::Fill)
        .build();

    let margin = if compact { 48 } else { 80 };
    let heading = Label::builder().label("All Apps").build();
    heading.add_css_class("section-label");
    heading.set_halign(Align::Start);
    heading.set_margin_start(margin);
    container.append(&heading);

    let scroller = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .build();

    // Exactly `per_row` children per line so the d-pad grid math in
    // move_grid() matches the visual layout.
    let flow = FlowBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .homogeneous(true)
        .row_spacing(20)
        .column_spacing(20)
        .min_children_per_line(per_row as u32)
        .max_children_per_line(per_row as u32)
        .halign(Align::Fill)
        .valign(Align::Start)
        .build();
    flow.set_margin_start(margin);
    flow.set_margin_end(margin);

    let mut tiles = Vec::with_capacity(entries.len());
    for entry in entries {
        let tile = build_app_tile(entry, compact);
        flow.append(&tile);
        tiles.push(tile);
    }
    scroller.set_child(Some(&flow));
    container.append(&scroller);
    (container, tiles)
}

fn build_mode_tile(mode: &Tile, compact: bool) -> Button {
    let tile = Button::builder().build();
    tile.add_css_class("tile");
    tile.add_css_class("mode-tile");

    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();

    let icon = Image::builder()
        .icon_name(&mode.icon)
        .pixel_size(if compact { 56 } else { 96 })
        .build();
    icon.add_css_class("tile-icon");
    content.append(&icon);

    let label = Label::builder().label(&mode.label).build();
    label.add_css_class("tile-label");
    content.append(&label);

    tile.set_child(Some(&content));
    tile
}

fn build_context_popover(
    parent: &gtk4::Widget,
    actions: &[ContextAction],
    position: PositionType,
) -> (Popover, Vec<Button>) {
    let popover = Popover::builder()
        .has_arrow(true)
        .position(position)
        .autohide(false)
        .build();
    popover.add_css_class("context-menu");
    popover.set_parent(parent);

    let column = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(4)
        .build();

    let header = Label::builder().label("Options").build();
    header.add_css_class("context-header");
    header.set_halign(Align::Start);
    column.append(&header);

    let mut buttons = Vec::with_capacity(actions.len());
    for action in actions {
        let btn = Button::builder().build();
        btn.add_css_class("context-item");
        let row = GtkBox::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .halign(Align::Start)
            .build();
        let icon = Image::builder()
            .icon_name(action.icon())
            .pixel_size(24)
            .build();
        icon.add_css_class("context-icon");
        row.append(&icon);
        let label = Label::builder().label(action.label()).build();
        label.add_css_class("context-label");
        row.append(&label);
        btn.set_child(Some(&row));
        column.append(&btn);
        buttons.push(btn);
    }

    popover.set_child(Some(&column));
    (popover, buttons)
}

fn build_app_tile(entry: &AppEntry, compact: bool) -> Button {
    let tile = Button::builder().build();
    tile.add_css_class("tile");
    tile.add_css_class("app-tile");

    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .halign(Align::Center)
        .valign(Align::Center)
        .build();

    let icon = Image::builder()
        .pixel_size(if compact { 44 } else { 72 })
        .build();
    // Prefer an absolute icon path; otherwise let gtk's icon theme resolve
    // the name. Falls back to a generic placeholder if neither works.
    if let Some(path) = &entry.icon_path {
        if Path::new(path).exists() {
            icon.set_from_file(Some(path));
        }
    } else if let Some(name) = &entry.icon_name {
        icon.set_icon_name(Some(name));
    } else {
        icon.set_icon_name(Some("application-x-executable"));
    }
    icon.add_css_class("tile-icon");
    content.append(&icon);

    let label = Label::builder()
        .label(&entry.name)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(14)
        .build();
    label.add_css_class("tile-label");
    content.append(&label);

    tile.set_child(Some(&content));
    tile
}
