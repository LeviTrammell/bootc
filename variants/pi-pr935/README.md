# Raspberry Pi PR #935 Test Variant

This is a **test variant** for Raspberry Pi that uses bootupd PR #935's `extend-payload-to-esp` command instead of the bootupctl shim approach used in the main `pi` variant.

## Purpose

Test whether [bootupd PR #935](https://github.com/coreos/bootupd/pull/935) provides a cleaner solution for installing firmware files to the ESP partition without needing a custom bootupctl shim.

## What's Different

### Main Pi Variant
- Stages firmware to `/usr/lib/bootc-raspi-firmwares/`
- Uses a bootupctl shim that intercepts `bootupd backend install`
- Shim copies firmware files to ESP during installation

### PR #935 Test Variant
- Uses `bootupctl backend extend-payload-to-esp` command
- Should stage firmware to `/usr/lib/efi/firmware/{name}/{version}/`
- bootupd should automatically install firmware during `bootupd backend install`
- No shim needed!

## Building

```bash
# Build the container
task containers:pi-pr935

# Build the disk image
task images:pi-pr935

# Output will be in dist/pi-pr935/
```

## Testing Checklist

- [ ] Container builds successfully
- [ ] Firmware files are staged to `/usr/lib/efi/firmware/`
- [ ] Disk image builds successfully
- [ ] Firmware files appear in `/boot/efi/` on the disk image
- [ ] Image boots on Raspberry Pi
- [ ] USB gadget networking works
- [ ] System is functional

## What to Report

If testing is successful, report to [PR #935](https://github.com/coreos/bootupd/pull/935) or [Issue #766](https://github.com/coreos/bootupd/issues/766):

1. **Success/Failure**: Did the image build and boot?
2. **Firmware Installation**: Were all firmware files correctly installed to ESP?
3. **Simplification**: Is this approach simpler than the shim?
4. **Updates**: Does `bootupctl update` handle firmware updates correctly?

## Comparison Commands

```bash
# Check firmware staging in container
podman run --rm ghcr.io/levitrammell/pi-pr935 ls -R /usr/lib/efi/firmware/

# Compare with main pi variant
podman run --rm ghcr.io/levitrammell/pi ls -R /usr/lib/bootc-raspi-firmwares/

# Check what's in ESP on built image
sudo mount dist/pi-pr935/image/disk.raw2 /mnt
ls -la /mnt/boot/efi/
sudo umount /mnt
```

## Expected Outcome

If PR #935 works correctly:
- Firmware files should be in `/usr/lib/efi/firmware/` in the container
- Firmware files should be copied to `/boot/efi/` during image build
- The Pi should boot without needing a bootupctl shim
- This proves PR #935 provides a better solution for ARM firmware

## Notes

- Uses Fedora 43 (Rawhide) as suggested in the bootupd discussions
- Requires ARM64 build environment or emulation
- May need adjustments if PR #935 API changes
