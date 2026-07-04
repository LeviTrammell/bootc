#!/bin/bash
# Generate /boot/extlinux/extlinux.conf from the ostree BLS entries.
# U-Boot's distro boot (sysboot) reads extlinux.conf but does NOT read BLS
# entries directly, so we must translate.
#
# Runs at boot (update-extlinux.service) AND at shutdown after
# ostree-finalize-staged (update-extlinux-finalize.service). The shutdown
# run is the load-bearing one: finalizing a staged deployment rewrites the
# BLS entries and flips the /ostree/boot.0 <-> boot.1 alternation, so an
# extlinux.conf generated earlier in the boot points at a path that no
# longer exists and the next boot dies in ostree-prepare-root.
set -euo pipefail

BOOT_DIR="/boot"
EXTLINUX_DIR="${BOOT_DIR}/extlinux"
EXTLINUX_CONF="${EXTLINUX_DIR}/extlinux.conf"
BLS_DIR="${BOOT_DIR}/loader/entries"

# OGU-specific kernel args not present in BLS entries
OGU_ARGS="consoleblank=0 plymouth.enable=0 panic=10 fbcon=rotate:3,font:VGA8x8"

# Newest entry first (ostree-2.conf > ostree-1.conf). All entries become
# labels — the newest is the default, the rest are pickable from the
# U-Boot serial console as a rollback path when the default won't boot.
ENTRIES=$(ls -1 "${BLS_DIR}"/ostree-*.conf 2>/dev/null | sort -rV)

if [ -z "${ENTRIES}" ]; then
    echo "ERROR: No ostree BLS entry found in ${BLS_DIR}" >&2
    exit 1
fi

TMP=$(mktemp "${EXTLINUX_DIR}/.extlinux.conf.XXXXXX")
trap 'rm -f "${TMP}"' EXIT

{
    echo "default ostree-0"
    echo "timeout 30"
    IDX=0
    for ENTRY in ${ENTRIES}; do
        LINUX=$(grep -m1 '^linux ' "${ENTRY}" | sed 's/^linux //')
        INITRD=$(grep -m1 '^initrd ' "${ENTRY}" | sed 's/^initrd //')
        OPTIONS=$(grep -m1 '^options ' "${ENTRY}" | sed 's/^options //')
        if [ -z "${LINUX}" ]; then
            echo "ERROR: No 'linux' field in ${ENTRY}" >&2
            exit 1
        fi
        APPEND="${OPTIONS}"
        for arg in ${OGU_ARGS}; do
            key="${arg%%=*}"
            if ! echo "${APPEND}" | grep -q "${key}"; then
                APPEND="${APPEND} ${arg}"
            fi
        done
        echo ""
        echo "label ostree-${IDX}"
        echo "    kernel ${LINUX}"
        echo "    initrd ${INITRD}"
        echo "    fdtdir /dtb"
        echo "    append ${APPEND}"
        IDX=$((IDX + 1))
    done
} > "${TMP}"

mkdir -p "${EXTLINUX_DIR}"
mv "${TMP}" "${EXTLINUX_CONF}"
trap - EXIT

echo "Updated ${EXTLINUX_CONF} from $(echo "${ENTRIES}" | wc -l) BLS entries:"
cat "${EXTLINUX_CONF}"
