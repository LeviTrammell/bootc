# Odroid Go Ultra Bootc Variant

This variant creates a bootable container image for the Odroid Go Ultra gaming handheld device.

## Hardware Specifications
- **SoC**: Amlogic S922X (Quad-core Cortex-A73 + Dual-core Cortex-A53)
- **RAM**: 2GB LPDDR4
- **Storage**: 16GB eMMC + MicroSD slot
- **Display**: 5-inch 854×480 MIPI-DSI TFT LCD
- **Battery**: Li-Polymer 3.7V/4000mAh

## Features
- Fedora 42-based bootc container
- U-Boot bootloader built from source
- Display validation on boot (shows system info)
- Gamepad input support
- Power management for battery operation
- Automatic partition resize on first boot
- SSH access enabled

## Building

### Build the container image:
```bash
task containers:odroid-go-ultra
```

### Build a bootable disk image (optional):
```bash
task images:odroid-go-ultra
```

## Installation

### Method 1: Direct to device (recommended)

1. Boot the Odroid Go Ultra into recovery mode or from another OS
2. Install the bootc container to disk:
```bash
podman run --rm --privileged --pid=host \
  -v /var/lib/containers:/var/lib/containers \
  -v /dev:/dev \
  --security-opt label=type:unconfined_t \
  ghcr.io/levitrammell/odroid-go-ultra \
  bootc install to-disk /dev/mmcblk0
```

3. Install the bootloader (required):
```bash
podman run --rm --privileged \
  -v /dev:/dev \
  --security-opt label=type:unconfined_t \
  ghcr.io/levitrammell/odroid-go-ultra \
  /usr/local/bin/install-bootloader /dev/mmcblk0
```

### Method 2: Manual bootloader installation

After running `bootc install to-disk`, manually write the bootloader:
```bash
dd if=/usr/lib/boot-firmware/odroid-go-ultra/1.0.0/boot/u-boot.bin.sd.bin \
   of=/dev/mmcblk0 conv=fsync,notrunc bs=512 seek=2048
```

## Display Validation

On successful boot, the device display will show:
- Boot success message in green
- System information (kernel version, hostname)
- Network interfaces and IP addresses
- Disk usage
- CPU and memory information
- "System Ready!" message

This confirms the device has booted successfully and is accessible via SSH.

## SSH Access

Default user configuration is set in `config.toml`. SSH is enabled by default.
Connect using the IP address shown on the display.

## Troubleshooting

### No display output
- Check U-Boot configuration and device tree
- Verify display driver modules are loaded: `lsmod | grep meson`
- Check kernel parameters in `/boot/extlinux/extlinux.conf`

### Input not working
- Check udev rules: `ls /etc/udev/rules.d/`
- Verify input devices: `ls /dev/input/`
- Test with `evtest` tool

### Boot issues
- Connect serial console to UART pins
- Check U-Boot output at 115200 baud
- Verify bootloader installation at correct offset

## Future Improvements

Once bootupd implements board-specific firmware support (issue #959), the manual bootloader installation step will be automated.