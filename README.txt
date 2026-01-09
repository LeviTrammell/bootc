Odroid Go Ultra U-Boot
Version: v2025.01
Built: Fri Oct 10 21:06:29 UTC 2025

Files:
  u-boot.bin.sd.bin - Signed bootloader for SD/eMMC (install at 1MB offset)
  u-boot-raw.bin - Raw U-Boot binary
  meson-g12b-odroid-go-ultra.dtb - Device tree blob

Installation:
  dd if=u-boot.bin.sd.bin of=/dev/mmcblk0 bs=512 seek=2048 conv=notrunc
