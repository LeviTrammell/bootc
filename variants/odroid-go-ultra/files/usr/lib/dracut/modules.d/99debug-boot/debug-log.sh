#!/bin/sh
# Write boot debug info to the FAT32 boot partition
# This script runs at multiple dracut hook points

BOOTDEV=""
for dev in /dev/mmcblk0p1 /dev/mmcblk1p1; do
    [ -b "$dev" ] && BOOTDEV="$dev" && break
done

[ -z "$BOOTDEV" ] && exit 0

mkdir -p /tmp/bootdebug
mount -t vfat "$BOOTDEV" /tmp/bootdebug 2>/dev/null || exit 0

PHASE="unknown"
case "$0" in
    *pre-mount*) PHASE="pre-mount" ;;
    *pre-pivot*) PHASE="pre-pivot" ;;
    *timeout*)   PHASE="initqueue-timeout" ;;
esac

{
    echo "========================================="
    echo "DEBUG LOG - Phase: $PHASE"
    echo "Time: $(cat /proc/uptime)"
    echo "========================================="
    echo ""
    echo "=== KERNEL CMDLINE ==="
    cat /proc/cmdline
    echo ""
    echo "=== BLOCK DEVICES ==="
    ls -la /dev/mmcblk* 2>&1
    echo ""
    ls -la /dev/disk/by-uuid/ 2>&1
    echo ""
    echo "=== MOUNTS ==="
    mount
    echo ""
    echo "=== SYSROOT ==="
    ls -la /sysroot/ 2>&1
    echo ""
    ls -la /sysroot/ostree/ 2>&1
    echo ""
    ls -la /sysroot/ostree/deploy/ 2>&1
    echo ""
    ls -la /sysroot/ostree/boot/ 2>&1
    echo ""
    echo "=== DMESG (last 200 lines) ==="
    dmesg | tail -200
    echo ""
} >> /tmp/bootdebug/debug.log 2>&1

sync
umount /tmp/bootdebug 2>/dev/null
