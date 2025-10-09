# Odroid Go Ultra Bootc Variant

Fedora bootc image for the Odroid Go Ultra handheld gaming device powered by Amlogic S922X SoC.

## Hardware Specifications
- **SoC**: Amlogic S922X (Quad-core Cortex-A73 + Dual-core Cortex-A53)
- **RAM**: 2GB LPDDR4
- **Storage**: 16GB eMMC + MicroSD slot
- **Display**: 5-inch 854×480 MIPI-DSI TFT LCD
- **Battery**: Li-Polymer 3.7V/4000mAh

## Architecture

This variant uses a **split build architecture** for clean separation of concerns:

```
┌───────────────────────────────┐
│ Containerfile.uboot           │  ← U-Boot builder (Ubuntu 22.04)
│ - Builds U-Boot v2025.01      │
│ - Signs with Amlogic FIP      │
│ - Outputs to scratch          │
└───────────────────────────────┘
           ↓
┌───────────────────────────────┐
│ Containerfile                 │  ← Fedora bootc image
│ - Pulls U-Boot artifacts      │
│ - Installs packages           │
│ - Configures system           │
└───────────────────────────────┘
           ↓
┌───────────────────────────────┐
│ bootc-image-builder           │  ← Disk image builder
│ - Creates partitions          │
│ - Installs ostree             │
└───────────────────────────────┘
           ↓
┌───────────────────────────────┐
│ task *-complete               │  ← Post-processing
│ - Injects bootloader at 1MB   │
│ - Ready-to-flash image        │
└───────────────────────────────┘
```

### Why Split Architecture?

**Fast Iteration:**
- U-Boot build (30min) is cached and reused
- Fedora config changes rebuild in minutes
- No need to rebuild U-Boot for system changes

**Reusability:**
- Same U-Boot container works for multiple images
- Can be shared across Amlogic devices (N2+, C4, etc.)
- Pattern applicable to other ARM SBCs

**Clean Separation:**
- U-Boot (Ubuntu, cross-compile) isolated from Fedora
- Bootloader updates independent of system config
- Easier to debug and maintain

## Building

### Quick Build (Recommended)

```bash
# One command - builds everything and produces flashable image
task images:odroid-go-ultra-complete
```

Output: `dist/odroid-go-ultra/image/disk.raw` (ready to flash!)

### Step-by-Step Build

```bash
# 1. Build U-Boot container (slow ~30min, but cacheable)
task containers:odroid-go-ultra-uboot

# 2. Build Fedora container (fast, uses U-Boot from step 1)
task containers:odroid-go-ultra

# 3. Build disk image
task images:odroid-go-ultra

# 4. Inject bootloader - creates complete flashable image
task images:odroid-go-ultra-complete
```

### Development Workflow

**Iterating on system config:**
```bash
# Edit Fedora configuration
vim variants/odroid-go-ultra/Containerfile

# Fast rebuild (skips U-Boot)
task containers:odroid-go-ultra
task images:odroid-go-ultra-complete
```

**Updating U-Boot:**
```bash
# Edit U-Boot builder
vim variants/odroid-go-ultra/Containerfile.uboot

# Rebuild everything
task containers:odroid-go-ultra-uboot  # Slow
task containers:odroid-go-ultra
task images:odroid-go-ultra-complete
```

## Installation

### To eMMC (via Recovery Mode) - Recommended

1. Hold **R2 + L2** while powering on
2. Connect USB-C cable to computer
3. eMMC appears as USB mass storage
4. Flash the complete image:

```bash
# On Linux
sudo dd if=dist/odroid-go-ultra/image/disk.raw \
    of=/dev/sdX \
    bs=4M \
    status=progress \
    conv=fsync

# On macOS
sudo dd if=dist/odroid-go-ultra/image/disk.raw \
    of=/dev/rdiskN \
    bs=4m \
    status=progress
```

5. Disconnect and boot!

### To SD Card

```bash
# Same command, different device
sudo dd if=dist/odroid-go-ultra/image/disk.raw \
    of=/dev/mmcblk0 \
    bs=4M \
    status=progress \
    conv=fsync
```

## Boot Flow

```
1. Amlogic BootROM (in SoC)
   └─ Reads bootloader from offset 1MB

2. U-Boot (u-boot.bin.sd.bin at 1MB)
   ├─ ARM Trusted Firmware (bl2, bl30, bl31)
   ├─ U-Boot (bl33)
   └─ DDR training firmware

3. U-Boot reads /boot/extlinux/extlinux.conf
   ├─ Loads /vmlinuz (kernel)
   └─ Loads /dtb/meson-g12b-odroid-go-ultra.dtb

4. Boot Fedora
```

## Features

**Display:**
- 5" 854x480 LCD with custom EDID
- Boot success message shows system info
- Confirms network connectivity

**Storage:**
- Auto-detects eMMC (`/dev/mmcblk1`) or SD (`/dev/mmcblk0`)
- Automatic partition resize on first boot
- Works on both storage types

**Input:**
- Gamepad controls (no keyboard)
- udev rules for input devices
- All interactive tools use `-f/--force` flags

**Power:**
- CPU governor: `ondemand`
- Battery management service
- Optimized for portable use

**Graphics:**
- Panfrost (Mali-G52) GPU
- Mesa Vulkan/OpenGL drivers
- DRM/KMS support

## SSH Access

Default user configured in `config.toml`:

```bash
# IP shown on display
ssh levi@<ip-address>
```

## Troubleshooting

### Image won't boot

**Verify bootloader injection:**
```bash
dd if=dist/odroid-go-ultra/image/disk.raw bs=512 skip=2048 count=1 | xxd | head
# Should show FIP header, not zeros
```

**Re-flash in recovery mode:**
- Recovery mode always works
- Can't brick the device

### Can't enter recovery mode

- Hold **R2 + L2** *before* powering on
- Keep holding 5-10 seconds
- LED should indicate recovery

### Partition won't resize

```bash
# Check logs
journalctl -u growfs.service

# Manual resize
sudo growpart /dev/mmcblk1 2
sudo resize2fs /dev/mmcblk1p2
```

## Files

- `Containerfile` - Main Fedora image (uses U-Boot container)
- `Containerfile.uboot` - U-Boot builder (Ubuntu, outputs scratch)
- `config.toml` - User config, kernel args

## Related Devices

This architecture works for other Amlogic devices:
- Odroid N2/N2+
- Odroid C4
- Generic S905X/S922X devices

Changes needed:
- Device tree (`.dtb`)
- U-Boot defconfig
- Display configuration

## References

- [U-Boot Amlogic Boot Flow](https://docs.u-boot.org/en/latest/board/amlogic/boot-flow.html)
- [LibreELEC FIP Tools](https://github.com/LibreELEC/amlogic-boot-fip)
- [Odroid Go Ultra Wiki](https://wiki.odroid.com/odroid_go_ultra/)
- [Fedora bootc](https://docs.fedoraproject.org/en-US/bootc/)
