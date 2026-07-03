#!/bin/bash
# Auto-detect root device and partition number, then grow.
# With composefs, / is an overlay - the real block device is what backs
# /sysroot. Strip any ostree bind-mount suffix: /dev/mmcblk1p3[/ostree/...]
ROOT_DEV=$(findmnt -n -o SOURCE /sysroot 2>/dev/null | sed 's/\[.*//')
if [ -z "$ROOT_DEV" ] || [ ! -b "$ROOT_DEV" ]; then
    ROOT_DEV=$(findmnt -n -o SOURCE / | sed 's/\[.*//')
fi
PARTNUM=$(echo "$ROOT_DEV" | grep -oP 'p\K[0-9]+$')
ROOT_DISK=$(lsblk -no pkname "$ROOT_DEV")

if [ -z "$ROOT_DISK" ] || [ -z "$PARTNUM" ]; then
    echo "Could not detect root disk or partition number (dev=$ROOT_DEV)"
    exit 1
fi

echo "Growing partition ${PARTNUM} on /dev/${ROOT_DISK}..."
growpart "/dev/${ROOT_DISK}" "${PARTNUM}"
rc=$?
if [ "$rc" -eq 0 ]; then
    resize2fs "$ROOT_DEV"
elif [ "$rc" -eq 1 ]; then
    # growpart exit 1 = NOCHANGE (already grown) - not an error
    echo "Partition already at full size"
    exit 0
else
    exit "$rc"
fi
