#!/bin/bash
# Mirror the ostree BLS entries onto the FAT32 boot partition (P1,
# mounted at /boot/efi) as extlinux.conf + kernel payloads.
#
# The vendor U-Boot reads extlinux.conf ONLY from the bootable FAT32
# partition - it does not read ext4 (P2 /boot) and does not read BLS
# entries. The BLS entries on P2 stay the source of truth; this script
# copies each deployment's kernel/initramfs/dtb to P1 and generates an
# extlinux.conf with FAT-relative paths.
#
# Runs at boot (update-extlinux.service) AND at shutdown after
# ostree-finalize-staged (update-extlinux-finalize.service). The shutdown
# run is the load-bearing one: finalizing a staged deployment rewrites the
# BLS entries and flips the /ostree/boot.0 <-> boot.1 alternation, so an
# extlinux.conf generated earlier points at a path that no longer exists
# and the next boot dies in ostree-prepare-root.
set -euo pipefail

BLS_DIR="/boot/loader/entries"
ESP="/boot/efi"
EXTLINUX_DIR="${ESP}/extlinux"
EXTLINUX_CONF="${EXTLINUX_DIR}/extlinux.conf"
# Factory DTB written to the FAT root by create-full-image.sh - fallback
# when a deployment's payload dir carries no dtb of its own.
FACTORY_DTB="/meson-g12b-odroid-go-ultra.dtb"
DTB_NAME="amlogic/meson-g12b-odroid-go-ultra.dtb"

# OGU-specific kernel args not present in BLS entries
OGU_ARGS="consoleblank=0 plymouth.enable=0 panic=10 fbcon=rotate:3,font:VGA8x8"

if ! mountpoint -q "${ESP}"; then
    echo "ERROR: ${ESP} is not mounted - U-Boot config not updated" >&2
    exit 1
fi

# Newest entry first (ostree-2.conf > ostree-1.conf). All entries become
# labels - the newest is the default, the rest are pickable from the
# U-Boot serial console as a rollback path when the default won't boot.
ENTRIES=$(ls -1 "${BLS_DIR}"/ostree-*.conf 2>/dev/null | sort -rV)

if [ -z "${ENTRIES}" ]; then
    echo "ERROR: No ostree BLS entry found in ${BLS_DIR}" >&2
    exit 1
fi

mkdir -p "${EXTLINUX_DIR}" "${ESP}/ostree"

REFERENCED=""
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

        # BLS paths are relative to /boot (P2), e.g.
        #   /ostree/default-<bootcsum>/vmlinuz-6.12.12
        # Copy the payload dir to the same path on P1. The bootcsum in
        # the dir name changes whenever the payload changes, so an
        # existing dir is already up to date.
        SRC_DIR="/boot$(dirname "${LINUX}")"
        NAME=$(basename "$(dirname "${LINUX}")")
        DEST_DIR="${ESP}/ostree/${NAME}"
        if [ ! -d "${DEST_DIR}" ]; then
            cp -r "${SRC_DIR}" "${DEST_DIR}.tmp"
            mv "${DEST_DIR}.tmp" "${DEST_DIR}"
        fi
        REFERENCED="${REFERENCED} ${NAME}"

        if [ -f "${DEST_DIR}/dtb/${DTB_NAME}" ]; then
            FDT="/ostree/${NAME}/dtb/${DTB_NAME}"
        else
            FDT="${FACTORY_DTB}"
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
        echo "    kernel /ostree/${NAME}/$(basename "${LINUX}")"
        echo "    initrd /ostree/${NAME}/$(basename "${INITRD}")"
        echo "    fdt ${FDT}"
        echo "    append ${APPEND}"
        IDX=$((IDX + 1))
    done
} > "${TMP}"

mv "${TMP}" "${EXTLINUX_CONF}"
trap - EXIT

# Prune payload dirs no longer referenced by any BLS entry
for d in "${ESP}"/ostree/*/; do
    [ -d "${d}" ] || continue
    NAME=$(basename "${d}")
    case " ${REFERENCED} " in
        *" ${NAME} "*) ;;
        *) echo "Pruning stale payload ${NAME}"; rm -rf "${d}" ;;
    esac
done

echo "Updated ${EXTLINUX_CONF} from $(echo "${ENTRIES}" | wc -l) BLS entries:"
cat "${EXTLINUX_CONF}"
