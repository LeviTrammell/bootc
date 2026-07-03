//! HTPC gamepad device probing.

use anyhow::{Context, Result};
use evdev::Device;
use log::info;

pub fn open_gamepad() -> Result<Device> {
    let devices = evdev::enumerate().collect::<Vec<_>>();
    for (path, device) in &devices {
        info!(
            "Found input device: {} ({})",
            device.name().unwrap_or("unnamed"),
            path.display()
        );
    }

    // Match on 8BitDo (any model) + standard Xbox pad name patterns. The
    // 2.4GHz Ultimate dongle reports as "8BitDo Ultimate Wireless
    // Controller" via xpad; some firmware reports "Microsoft X-Box 360 pad".
    let patterns = [
        "8bitdo",
        "x-box 360",
        "xbox",
        "gamepad",
        "controller",
    ];

    for (path, device) in &devices {
        let name = device.name().unwrap_or("unnamed").to_lowercase();
        for pat in &patterns {
            if name.contains(pat) {
                info!(
                    "Selected gamepad: {} ({})",
                    device.name().unwrap_or("?"),
                    path.display()
                );
                return Device::open(path)
                    .with_context(|| format!("Failed to open {}", path.display()));
            }
        }
    }

    // Capability fallback for synthetic uinput devices used in VM tests.
    for (path, device) in &devices {
        let has_btn_south = device
            .supported_keys()
            .map(|k| k.contains(evdev::Key::BTN_SOUTH))
            .unwrap_or(false);
        let has_mode = device
            .supported_keys()
            .map(|k| k.contains(evdev::Key::BTN_MODE))
            .unwrap_or(false);
        if has_btn_south && has_mode {
            info!(
                "Selected device by caps: {} ({})",
                device.name().unwrap_or("?"),
                path.display()
            );
            return Device::open(path)
                .with_context(|| format!("Failed to open {}", path.display()));
        }
    }

    anyhow::bail!(
        "No HTPC gamepad device found. Available: {}",
        devices
            .iter()
            .map(|(p, d)| format!("{} ({})", d.name().unwrap_or("?"), p.display()))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
