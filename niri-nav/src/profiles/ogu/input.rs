//! OGU-specific gamepad device probing.

use anyhow::{Context, Result};
use evdev::Device;
use log::info;

pub fn open_gamepad() -> Result<Device> {
    let devices = evdev::enumerate().collect::<Vec<_>>();

    for (path, device) in &devices {
        let name = device.name().unwrap_or("unnamed");
        info!("Found input device: {} ({})", name, path.display());
    }

    // OGU candidate names — observed on Fedora aarch64 with the meson DT.
    let patterns = [
        "odroid",
        "gamepad",
        "joystick",
        "joypad",
        "Go Ultra",
        "gpio-keys",
        "adc-keys",
        "input-keys",
    ];

    for (path, device) in &devices {
        let name = device.name().unwrap_or("unnamed").to_lowercase();
        for pat in &patterns {
            if name.contains(&pat.to_lowercase()) {
                info!(
                    "Selected gamepad: {} ({})",
                    device.name().unwrap_or("?"),
                    path.display()
                );
                let dev = Device::open(path)
                    .with_context(|| format!("Failed to open {}", path.display()))?;
                return Ok(dev);
            }
        }
    }

    // Capability-based fallback for VM/test boxes.
    for (path, device) in &devices {
        let name = device.name().unwrap_or("unnamed");
        let has_abs_hat = device
            .supported_absolute_axes()
            .map(|axes| axes.contains(evdev::AbsoluteAxisType::ABS_HAT0X))
            .unwrap_or(false);
        let has_btn_south = device
            .supported_keys()
            .map(|keys| keys.contains(evdev::Key::BTN_SOUTH))
            .unwrap_or(false);
        if has_abs_hat || has_btn_south {
            info!("Selected device by caps: {} ({})", name, path.display());
            let dev = Device::open(path)
                .with_context(|| format!("Failed to open {}", path.display()))?;
            return Ok(dev);
        }
    }

    anyhow::bail!(
        "No OGU gamepad device found. Available: {}",
        devices
            .iter()
            .map(|(p, d)| format!("{} ({})", d.name().unwrap_or("?"), p.display()))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
