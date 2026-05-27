#!/bin/bash
# Auto-detect root device and partition number, then grow
# Strip ostree bind-mount path: /dev/mmcblk1p3[/ostree/...] -> /dev/mmcblk1p3
ROOT_DEV=$(findmnt -n -o SOURCE / | sed 's/\[.*//')
PARTNUM=$(echo "$ROOT_DEV" | grep -oP 'p\K[0-9]+$')
ROOT_DISK=$(lsblk -no pkname "$ROOT_DEV")

if [ -z "$ROOT_DISK" ] || [ -z "$PARTNUM" ]; then
    echo "Could not detect root disk or partition number (dev=$ROOT_DEV)"
    exit 1
fi

echo "Growing partition ${PARTNUM} on /dev/${ROOT_DISK}..."
growpart "/dev/${ROOT_DISK}" "${PARTNUM}" && resize2fs "$ROOT_DEV"
