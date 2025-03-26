#!/bin/bash
set -eux

if [ -f /run/grow-root-done ]; then
  echo "[grow-root] Already resized, skipping."
  exit 0
fi

echo "[grow-root] Growing partition and resizing filesystem..."

growpart /dev/mmcblk0 3
e2fsck -f -y /dev/mmcblk0p3
resize2fs /dev/mmcblk0p3

touch /run/grow-root-done
