#!/bin/bash
# cma-baseline.sh — capture the CMA / display / initramfs picture on the OGU.
#
# Run this on a *working* boot to establish the baseline, then again on a
# dracut test boot to compare. The whole dracut-vs-custom-init question
# hinges on these numbers:
#
#   - How big is the CMA region the kernel actually reserved?
#   - How much CMA is free at idle (with niri + panel up)?
#   - Where does CMA sit physically vs. where U-Boot loads the initrd?
#   - Did the MIPI-DSI connector come up clean?
#
# If CMA is large and mostly free, a 30MB initramfs "starving" it implies a
# placement/address collision (needs cma=/initrd_high). If CMA is small,
# raw consumption is the story and shrinking dracut is the fix.
#
# Usage:  sudo ./cma-baseline.sh            (run on device)
#    or:   ssh levi@10.55.0.1 'sudo bash -s' < cma-baseline.sh   (from host)

set -uo pipefail
sec() { printf '\n===== %s =====\n' "$1"; }

sec "uname / os"
uname -a
grep PRETTY /etc/os-release 2>/dev/null

sec "kernel cmdline (note any cma= / initrd handling)"
cat /proc/cmdline

sec "total RAM"
free -m

sec "CMA from /proc/meminfo (CmaTotal / CmaFree = the budget)"
grep -i cma /proc/meminfo || echo "(no Cma* lines — CMA may be disabled?)"

sec "CMA region as configured in the device tree (bytes, may be hex BE)"
for n in /proc/device-tree/reserved-memory/linux,cma* \
         /proc/device-tree/reserved-memory/*cma*; do
    [ -e "$n/size" ] || continue
    printf '%s/size = ' "$n"; od -An -tx1 "$n/size" | tr -d ' \n'; echo
    printf '%s/reg  = ' "$n";  od -An -tx1 "$n/reg"  2>/dev/null | tr -d ' \n'; echo
done

sec "CMA debugfs (region size in pages, current usage)"
if [ -d /sys/kernel/debug/cma ]; then
    for d in /sys/kernel/debug/cma/*/; do
        echo "[$d]"; for f in count used maxchunk; do
            [ -e "$d$f" ] && printf '  %s = %s\n' "$f" "$(cat "$d$f")"; done
    done
else
    echo "(debugfs cma not mounted — try: mount -t debugfs none /sys/kernel/debug)"
fi

sec "physical memory layout: CMA vs initrd vs reserved (dmesg, early)"
dmesg | grep -iE 'cma:|reserved|initrd|Memory:|Zone|crashkernel' | head -40

sec "physical iomem map (where CMA / reserved sit)"
grep -iE 'cma|reserved|System RAM' /proc/iomem 2>/dev/null | head -40

sec "current initramfs size on disk (per deployment)"
ls -la /usr/lib/modules/*/initramfs.img 2>/dev/null
ls -la /boot/ostree/*/initramfs* 2>/dev/null

sec "DRM connectors — did the DSI panel come up?"
for c in /sys/class/drm/*/status; do
    [ -e "$c" ] && printf '%s = %s\n' "$c" "$(cat "$c")"
done
ls -la /dev/dri/ 2>/dev/null

sec "display / GPU dmesg (meson-drm, dsi, panel, panfrost, cma alloc fails)"
dmesg | grep -iE 'meson|dsi|panel|drm|panfrost|cma_alloc|alloc.*fail|dma' | tail -50

sec "done"
echo "Capture complete. Compare CmaTotal/CmaFree + the cma: Reserved line"
echo "against a dracut test boot to decide: shrink (consumption) or place (cma=)."
