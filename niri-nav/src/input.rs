//! Shared evdev helpers (grab/ungrab). Device probing lives in each profile.

use anyhow::{Context, Result};
use evdev::Device;
use log::info;

pub fn grab(device: &mut Device) -> Result<()> {
    device.grab().context("Failed to grab device (EVIOCGRAB)")?;
    info!("Device grabbed");
    Ok(())
}

pub fn ungrab(device: &mut Device) -> Result<()> {
    device.ungrab().context("Failed to ungrab device")?;
    info!("Device ungrabbed");
    Ok(())
}
