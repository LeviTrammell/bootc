//! controller-shell — persistent gtk4-layer-shell overlay for niri-driven
//! variants (HTPC, OGU). Surfaces the mode picker today; will grow to
//! include the app launcher, quick menu, and power menu.
//!
//! The shell runs as a long-lived process. niri-nav (the gamepad event
//! router) holds the evdev grab and forwards intent-level commands to the
//! shell over stdin (one JSON object per line). The shell never reads the
//! gamepad itself — that keeps grab semantics simple and lets the shell
//! also accept keyboard input naturally from gtk for dev/iteration.
//!
//! Protocol (stdin, one line per command):
//!   {"cmd":"show"}        — animate the overlay in, focus restored
//!   {"cmd":"hide"}        — animate the overlay out
//!   {"cmd":"up"}          — focus previous in current axis
//!   {"cmd":"down"}        — focus next in current axis
//!   {"cmd":"left"}        — focus previous column / row
//!   {"cmd":"right"}       — focus next column / row
//!   {"cmd":"select"}      — activate focused item (launches app, etc.)
//!   {"cmd":"back"}        — pop to previous view, or hide if at root
//!   {"cmd":"view","name":"modes|apps"}  — jump to a view

mod app;
mod config;
mod desktop;
mod ipc;
mod ipc_server;
mod osk;

use anyhow::Result;
use gtk4::prelude::*;
use gtk4::Application;
use log::info;

const APP_ID: &str = "dev.bootc.ControllerShell";

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("controller-shell starting (gtk4-layer-shell)");

    let application = Application::builder().application_id(APP_ID).build();
    application.connect_activate(app::on_activate);

    let exit = application.run();
    std::process::exit(exit.value());
}
