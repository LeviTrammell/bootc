use anyhow::{Context, Result};
use evdev::Device;
use log::info;

/// Find and open the gamepad evdev device.
///
/// Scans /dev/input/event* for a device whose name matches the OGU gamepad.
/// The exact name will be confirmed after Phase 1 recon; for now we search
/// for common patterns.
pub fn open_gamepad() -> Result<Device> {
    let devices = evdev::enumerate()
        .collect::<Vec<_>>();

    for (path, device) in &devices {
        let name = device.name().unwrap_or("unnamed");
        info!("Found input device: {} ({})", name, path.display());
    }

    // Try to find the gamepad by name patterns (will be refined after recon)
    let gamepad_patterns = [
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
        for pattern in &gamepad_patterns {
            if name.contains(&pattern.to_lowercase()) {
                info!("Selected gamepad: {} ({})", device.name().unwrap_or("?"), path.display());
                // Re-open the device since we borrowed it from enumerate
                let dev = Device::open(path)
                    .with_context(|| format!("Failed to open {}", path.display()))?;
                return Ok(dev);
            }
        }
    }

    // If no pattern match, check for devices with gamepad-like capabilities
    for (path, device) in &devices {
        let name = device.name().unwrap_or("unnamed");
        let has_abs_hat = device.supported_absolute_axes()
            .map(|axes| axes.contains(evdev::AbsoluteAxisType::ABS_HAT0X))
            .unwrap_or(false);
        let has_btn_south = device.supported_keys()
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
        "No gamepad device found. Available devices: {}",
        devices.iter()
            .map(|(p, d)| format!("{} ({})", d.name().unwrap_or("?"), p.display()))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Grab exclusive access to the device (EVIOCGRAB).
pub fn grab(device: &mut Device) -> Result<()> {
    device.grab().context("Failed to grab device (EVIOCGRAB)")?;
    info!("Device grabbed");
    Ok(())
}

/// Release exclusive access to the device.
pub fn ungrab(device: &mut Device) -> Result<()> {
    device.ungrab().context("Failed to ungrab device")?;
    info!("Device ungrabbed");
    Ok(())
}
