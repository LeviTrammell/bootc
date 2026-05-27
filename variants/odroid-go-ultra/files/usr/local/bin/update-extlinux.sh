#!/bin/bash
# Generate /boot/extlinux/extlinux.conf from the current ostree BLS entry.
# U-Boot's distro boot (sysboot) reads extlinux.conf but does NOT read BLS
# entries directly, so we must translate.
set -euo pipefail

BOOT_DIR="/boot"
EXTLINUX_DIR="${BOOT_DIR}/extlinux"
EXTLINUX_CONF="${EXTLINUX_DIR}/extlinux.conf"

# Find the current BLS entry
BLS_DIR="${BOOT_DIR}/loader/entries"
BLS_ENTRY=$(ls -1t "${BLS_DIR}"/ostree-*.conf 2>/dev/null | head -1)

if [ -z "${BLS_ENTRY}" ]; then
    echo "ERROR: No ostree BLS entry found in ${BLS_DIR}" >&2
    exit 1
fi

echo "Generating extlinux.conf from: ${BLS_ENTRY}"

# Parse BLS entry fields
TITLE=$(grep -m1 '^title ' "${BLS_ENTRY}" | sed 's/^title //')
LINUX=$(grep -m1 '^linux ' "${BLS_ENTRY}" | sed 's/^linux //')
INITRD=$(grep -m1 '^initrd ' "${BLS_ENTRY}" | sed 's/^initrd //')
OPTIONS=$(grep -m1 '^options ' "${BLS_ENTRY}" | sed 's/^options //')

if [ -z "${LINUX}" ]; then
    echo "ERROR: No 'linux' field in BLS entry" >&2
    exit 1
fi

# OGU-specific kernel args not present in BLS entries
OGU_ARGS="consoleblank=0 plymouth.enable=0 panic=10 fbcon=rotate:3,font:VGA8x8"

# Append OGU args (skip any already present in OPTIONS)
APPEND="${OPTIONS}"
for arg in ${OGU_ARGS}; do
    key="${arg%%=*}"
    if ! echo "${APPEND}" | grep -q "${key}"; then
        APPEND="${APPEND} ${arg}"
    fi
done

# Write extlinux.conf
mkdir -p "${EXTLINUX_DIR}"
cat > "${EXTLINUX_CONF}" <<EOF
default ostree
timeout 30

label ostree
    kernel ${LINUX}
    initrd ${INITRD}
    fdtdir /dtb
    append ${APPEND}
EOF

echo "Updated ${EXTLINUX_CONF}:"
cat "${EXTLINUX_CONF}"
