#!/bin/bash
# Create a complete flashable image for the ODROID Go Ultra
#
# This script post-processes the bootc-image-builder output to:
# 1. Inject the vendor U-Boot FIP at sector 1
# 2. Set MBR flags (bootable + FAT32 LBA type)
# 3. Copy kernel Image, DTBs, and initramfs to the FAT32 boot partition
# 4. Generate extlinux.conf for U-Boot's distro boot
#
# Prerequisites:
# - mtools (brew install mtools on macOS)
# - python3
# - The kernel and U-Boot containers must be built first
#
# Usage:
#   ./create-full-image.sh [CONTAINER_IMAGE] [UBOOT_IMAGE] [KERNEL_IMAGE]

set -e

CONTAINER_IMAGE="${1:-ghcr.io/levitrammell/odroid-go-ultra}"
UBOOT_IMAGE="${2:-ghcr.io/levitrammell/odroid-go-ultra-uboot}"
KERNEL_IMAGE="${3:-ghcr.io/levitrammell/odroid-go-ultra-kernel}"
OUTPUT_DIR="dist/odroid-go-ultra"
WORK_DIR="${OUTPUT_DIR}/work"
FINAL_IMAGE="${OUTPUT_DIR}/image/disk.raw"

echo "=== ODROID Go Ultra Image Builder ==="
echo "Container: ${CONTAINER_IMAGE}"
echo "U-Boot:    ${UBOOT_IMAGE}"
echo "Kernel:    ${KERNEL_IMAGE}"
echo ""

# Create work directory
mkdir -p "${WORK_DIR}"

# ─── Step 1: Extract U-Boot FIP from container ───
echo "Step 1: Extracting U-Boot FIP..."
UBOOT_CONTAINER=$(podman create "${UBOOT_IMAGE}" 2>/dev/null) || {
    echo "ERROR: Could not create container from ${UBOOT_IMAGE}"
    echo "Build it first: task containers:odroid-go-ultra-uboot"
    exit 1
}
podman cp "${UBOOT_CONTAINER}:/u-boot.bin" "${WORK_DIR}/u-boot.bin" 2>/dev/null || {
    # Fallback: try .sd.bin (mainline format)
    podman cp "${UBOOT_CONTAINER}:/u-boot.bin.sd.bin" "${WORK_DIR}/u-boot.bin.sd.bin" 2>/dev/null || {
        echo "ERROR: No U-Boot binary found in container"
        podman rm -f "${UBOOT_CONTAINER}" >/dev/null 2>&1
        exit 1
    }
}
# Extract splash screen BMPs if available
mkdir -p "${WORK_DIR}/res"
podman cp "${UBOOT_CONTAINER}:/res/." "${WORK_DIR}/res/" 2>/dev/null || true
podman rm -f "${UBOOT_CONTAINER}" >/dev/null 2>&1
echo "  U-Boot extracted: $(du -h ${WORK_DIR}/u-boot.bin* | tail -1 | cut -f1)"

# ─── Step 2: Extract kernel files from container ───
echo "Step 2: Extracting kernel files..."
KERNEL_CONTAINER=$(podman create "${KERNEL_IMAGE}" 2>/dev/null) || {
    echo "ERROR: Could not create container from ${KERNEL_IMAGE}"
    echo "Build it first: task containers:odroid-go-ultra-kernel"
    exit 1
}
mkdir -p "${WORK_DIR}/kernel"
podman cp "${KERNEL_CONTAINER}:/boot/Image" "${WORK_DIR}/kernel/Image"
podman cp "${KERNEL_CONTAINER}:/boot/dtb" "${WORK_DIR}/kernel/dtb"
podman cp "${KERNEL_CONTAINER}:/kernel-version" "${WORK_DIR}/kernel/kernel-version"
podman rm -f "${KERNEL_CONTAINER}" >/dev/null 2>&1

KVER=$(cat "${WORK_DIR}/kernel/kernel-version")
KERNEL_SIZE=$(stat -f%z "${WORK_DIR}/kernel/Image" 2>/dev/null || stat -c%s "${WORK_DIR}/kernel/Image")
KERNEL_MB=$((KERNEL_SIZE / 1024 / 1024))
echo "  Kernel: ${KERNEL_MB}MB (version ${KVER})"
if [ "${KERNEL_MB}" -gt 30 ]; then
    echo "  WARNING: Kernel is ${KERNEL_MB}MB - may exceed vendor U-Boot's ~30MB limit"
fi

# ─── Step 3: Check disk image exists ───
echo "Step 3: Checking disk image..."
if [ ! -f "${FINAL_IMAGE}" ]; then
    echo "  Disk image not found at ${FINAL_IMAGE}"
    echo "  Building it now with bootc-image-builder..."
    task images:odroid-go-ultra
fi
echo "  Disk image: $(du -h ${FINAL_IMAGE} | cut -f1)"

# ─── Step 4: Read partition table and root UUID ───
echo "Step 4: Reading partition table..."
PART_INFO=$(python3 -c "
import struct
with open('${FINAL_IMAGE}', 'rb') as f:
    f.seek(446)
    for i in range(4):
        entry = f.read(16)
        status, _, ptype, _, lba_start, lba_size = struct.unpack('<B3sB3sII', entry)
        if ptype != 0:
            print(f'{i+1} {status} {ptype} {lba_start} {lba_size}')
")

P1_START=$(echo "${PART_INFO}" | head -1 | awk '{print $4}')
P1_SECTORS=$(echo "${PART_INFO}" | head -1 | awk '{print $5}')
P1_SIZE_BYTES=$((P1_SECTORS * 512))

echo "  P1: start=${P1_START} sectors=${P1_SECTORS} (${P1_SIZE_BYTES} bytes)"

# Find the root partition (last Linux partition = P3) and read its UUID
ROOT_UUID=$(python3 -c "
import struct
with open('${FINAL_IMAGE}', 'rb') as f:
    # Find the last Linux (0x83) partition
    root_start = 0
    f.seek(446)
    for i in range(4):
        entry = f.read(16)
        _, _, ptype, _, lba_start, _ = struct.unpack('<B3sB3sII', entry)
        if ptype == 0x83:
            root_start = lba_start
    if root_start == 0:
        raise SystemExit('No Linux partition found')
    # Read ext4 superblock UUID (offset 1024+104 from partition start)
    # UUID is raw bytes, no endian swapping (matches blkid output)
    f.seek(root_start * 512 + 1024 + 104)
    u = f.read(16)
    print(f'{u[0]:02x}{u[1]:02x}{u[2]:02x}{u[3]:02x}-{u[4]:02x}{u[5]:02x}-{u[6]:02x}{u[7]:02x}-{u[8]:02x}{u[9]:02x}-{u[10]:02x}{u[11]:02x}{u[12]:02x}{u[13]:02x}{u[14]:02x}{u[15]:02x}')
")
echo "  Root UUID: ${ROOT_UUID}"

# ─── Step 5: Inject U-Boot FIP at sector 1 ───
echo "Step 5: Injecting U-Boot FIP at sector 1..."
if [ -f "${WORK_DIR}/u-boot.bin.sd.bin" ]; then
    # Mainline format: .sd.bin has 512-byte prefix, skip it
    dd if="${WORK_DIR}/u-boot.bin.sd.bin" of="${FINAL_IMAGE}" bs=512 skip=1 seek=1 conv=notrunc 2>/dev/null
    # Write MBR boot code (first 440 bytes)
    dd if="${WORK_DIR}/u-boot.bin.sd.bin" of="${FINAL_IMAGE}" bs=1 count=440 conv=notrunc 2>/dev/null
elif [ -f "${WORK_DIR}/u-boot.bin" ]; then
    # ROCKNIX/vendor format: raw FIP, write at sector 1
    dd if="${WORK_DIR}/u-boot.bin" of="${FINAL_IMAGE}" bs=512 seek=1 conv=notrunc 2>/dev/null
fi
echo "  FIP injected"

# ─── Step 6: Fix MBR partition flags ───
echo "Step 6: Setting MBR flags (P1 bootable + FAT32 LBA)..."
python3 -c "
import struct

with open('${FINAL_IMAGE}', 'r+b') as f:
    # Set P1 status to bootable (0x80)
    f.seek(446)
    f.write(struct.pack('B', 0x80))

    # Set P1 type to FAT32 LBA (0x0C)
    f.seek(446 + 4)
    f.write(struct.pack('B', 0x0C))

    # Verify MBR signature
    f.seek(510)
    sig = f.read(2)
    if sig != b'\x55\xaa':
        f.seek(510)
        f.write(b'\x55\xaa')
        print('  Fixed MBR signature')

    # Verify @AML bootloader presence
    f.seek(512 + 16)
    tag = f.read(4)
    if tag == b'@AML':
        print('  Verified: @AML bootloader present at sector 1')
    else:
        print('  WARNING: @AML signature not found at sector 1')
"

# ─── Step 7: Build FAT32 boot partition ───
echo "Step 7: Building FAT32 boot partition..."

# Create a FAT32 filesystem image matching P1 size
FAT_IMG="${WORK_DIR}/boot.fat32"
dd if=/dev/zero of="${FAT_IMG}" bs=512 count="${P1_SECTORS}" 2>/dev/null
mkfs.fat -F 32 -n BOOT "${FAT_IMG}" >/dev/null 2>&1

# Copy kernel Image
mcopy -i "${FAT_IMG}" "${WORK_DIR}/kernel/Image" ::Image
echo "  Copied kernel Image (${KERNEL_MB}MB)"

# Copy DTBs to partition root (ROCKNIX style - U-Boot finds them with FDTDIR /)
for dtb in "${WORK_DIR}"/kernel/dtb/amlogic/meson-g12b-*.dtb; do
    if [ -f "${dtb}" ]; then
        mcopy -i "${FAT_IMG}" "${dtb}" "::$(basename ${dtb})"
        echo "  Copied $(basename ${dtb})"
    fi
done

# Try to get initramfs from the system container
echo "  Extracting initramfs from system container..."
SYS_CONTAINER=$(podman create "${CONTAINER_IMAGE}" 2>/dev/null) || true
if [ -n "${SYS_CONTAINER}" ]; then
    # Find and copy initramfs
    INITRAMFS_PATH=$(podman run --rm "${CONTAINER_IMAGE}" bash -c 'ls /usr/lib/modules/*/initramfs.img 2>/dev/null | head -1' 2>/dev/null) || true
    if [ -n "${INITRAMFS_PATH}" ]; then
        podman cp "${SYS_CONTAINER}:${INITRAMFS_PATH}" "${WORK_DIR}/kernel/initramfs.img" 2>/dev/null || true
    fi
    podman rm -f "${SYS_CONTAINER}" >/dev/null 2>&1
fi

if [ -f "${WORK_DIR}/kernel/initramfs.img" ]; then
    mcopy -i "${FAT_IMG}" "${WORK_DIR}/kernel/initramfs.img" ::initramfs.img
    INITRD_SIZE=$(stat -f%z "${WORK_DIR}/kernel/initramfs.img" 2>/dev/null || stat -c%s "${WORK_DIR}/kernel/initramfs.img")
    echo "  Copied initramfs ($((INITRD_SIZE / 1024 / 1024))MB)"
    INITRD_LINE="  INITRD /initramfs.img"
else
    echo "  WARNING: No initramfs found - booting without initrd"
    INITRD_LINE=""
fi

# Generate extlinux.conf
mmd -i "${FAT_IMG}" ::extlinux 2>/dev/null || true

EXTLINUX_CONTENT="default fedora
timeout 30

label fedora
  LINUX /Image
${INITRD_LINE}
  FDT /meson-g12b-odroid-go-ultra.dtb
  APPEND rdinit=/init root=UUID=${ROOT_UUID} rw rootwait panic=10 clk_ignore_unused console=ttyAML0,115200n8 console=tty0 no_console_suspend fbcon=rotate:1 consoleblank=0 plymouth.enable=0"

echo "${EXTLINUX_CONTENT}" > "${WORK_DIR}/extlinux.conf"
mcopy -i "${FAT_IMG}" "${WORK_DIR}/extlinux.conf" ::extlinux/extlinux.conf
echo "  Generated extlinux.conf"

# Copy splash screen BMPs (vendor U-Boot displays these)
if [ -d "${WORK_DIR}/res" ] && [ "$(ls -A ${WORK_DIR}/res/ 2>/dev/null)" ]; then
    mmd -i "${FAT_IMG}" ::res 2>/dev/null || true
    for bmp in "${WORK_DIR}"/res/*.bmp; do
        [ -f "${bmp}" ] && mcopy -i "${FAT_IMG}" "${bmp}" ::res/
    done
    echo "  Copied splash screen BMPs"
fi

# Show FAT32 contents
echo "  FAT32 partition contents:"
mdir -i "${FAT_IMG}" :: 2>/dev/null | grep -v "^$" | sed 's/^/    /'

# ─── Step 8: Write FAT32 to disk image ───
echo "Step 8: Writing FAT32 boot partition to disk image..."
dd if="${FAT_IMG}" of="${FINAL_IMAGE}" bs=512 seek="${P1_START}" conv=notrunc 2>/dev/null
echo "  Written ${P1_SECTORS} sectors at offset ${P1_START}"

# ─── Step 9: Final verification ───
echo ""
echo "Step 9: Final verification..."
python3 << PYEOF
import struct

disk = '${FINAL_IMAGE}'
with open(disk, 'rb') as f:
    # MBR partition table
    print('  MBR Partition Table:')
    type_names = {0x0c: 'FAT32-LBA', 0x83: 'Linux', 0x06: 'FAT16', 0x0b: 'FAT32'}
    for i in range(4):
        f.seek(446 + i*16)
        entry = f.read(16)
        status, _, ptype, _, lba_start, lba_size = struct.unpack('<B3sB3sII', entry)
        if ptype != 0:
            size_mb = lba_size * 512 / 1024 / 1024
            tname = type_names.get(ptype, '0x{:02x}'.format(ptype))
            boot = 'bootable' if status == 0x80 else ''
            print(f'    P{i+1}: {tname} start={lba_start} {size_mb:.0f}MB {boot}')

    # Bootloader
    f.seek(512 + 16)
    tag = f.read(4)
    aml = 'PRESENT' if tag == b'@AML' else 'MISSING'
    print(f'  Bootloader: @AML {aml}')

    # FAT32 check
    f.seek(446)
    entry = f.read(16)
    _, _, _, _, p1_start, _ = struct.unpack('<B3sB3sII', entry)
    f.seek(p1_start * 512 + 82)
    fs_type = f.read(8).decode('ascii', errors='replace').strip()
    print(f'  P1 filesystem: {fs_type}')
PYEOF

# ─── Step 10: Clean up and compress ───
echo ""
echo "Step 10: Compressing image..."
rm -rf "${WORK_DIR}"

if command -v gzip &> /dev/null; then
    gzip -k -f "${FINAL_IMAGE}"
    COMPRESSED="${FINAL_IMAGE}.gz"
    echo "  Compressed: $(du -h ${COMPRESSED} | cut -f1)"
fi

echo ""
echo "=== Image build complete ==="
echo "  Image: ${FINAL_IMAGE}"
echo "  Size:  $(du -h ${FINAL_IMAGE} | cut -f1)"
echo ""
echo "To flash:"
echo "  sudo diskutil unmountDisk diskN"
echo "  sudo dd if=${FINAL_IMAGE} of=/dev/rdiskN bs=4m"
