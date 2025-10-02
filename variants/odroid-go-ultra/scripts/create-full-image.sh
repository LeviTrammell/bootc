#!/bin/bash
# Script to create a complete flashable image with embedded bootloader
# This combines the disk.raw and u-boot.bin.sd.bin into a single image

set -e

echo "Creating complete Odroid Go Ultra image with embedded bootloader..."

# Check if running on macOS or Linux
if [[ "$OSTYPE" == "darwin"* ]]; then
    echo "Running on macOS"
    # macOS uses different dd syntax
    DD_CONV="conv=notrunc"
else
    echo "Running on Linux"
    DD_CONV="conv=notrunc,fsync"
fi

# Variables
CONTAINER_IMAGE="${1:-ghcr.io/levitrammell/odroid-go-ultra}"
OUTPUT_DIR="dist/odroid-go-ultra"
WORK_DIR="${OUTPUT_DIR}/work"
FINAL_IMAGE="${OUTPUT_DIR}/odroid-go-ultra-complete.img"

# Create work directory
mkdir -p "${WORK_DIR}"

# Step 1: Extract bootloader from container
echo "Extracting bootloader from container..."

# Try to extract from main container first
if podman create --name temp-ogu "${CONTAINER_IMAGE}" 2>/dev/null; then
    if podman cp temp-ogu:/usr/lib/boot-firmware/odroid-go-ultra/1.0.0/boot/u-boot.bin.sd.bin "${WORK_DIR}/u-boot.bin.sd.bin" 2>/dev/null; then
        echo "Extracted bootloader from main container"
    else
        echo "Bootloader not found in main container, trying U-Boot container..."
        podman rm -f temp-ogu
        # Try the dedicated U-Boot container
        UBOOT_IMAGE="${CONTAINER_IMAGE}-uboot"
        if podman create --name temp-uboot "${UBOOT_IMAGE}" 2>/dev/null; then
            podman cp temp-uboot:/u-boot.bin.sd.bin "${WORK_DIR}/u-boot.bin.sd.bin"
            podman rm -f temp-uboot
            echo "Extracted bootloader from U-Boot container"
        else
            echo "Error: Could not find bootloader in either container"
            exit 1
        fi
    fi
    podman rm -f temp-ogu 2>/dev/null
fi

# Step 2: Check if disk image exists
if [ ! -f "${OUTPUT_DIR}/image/disk.raw" ]; then
    echo "Disk image not found. Building it now..."
    task images:odroid-go-ultra
fi

# Step 3: Create a copy of the disk image
echo "Creating complete image..."
cp "${OUTPUT_DIR}/image/disk.raw" "${FINAL_IMAGE}"

# Step 4: Embed bootloader into the image at correct offset
echo "Embedding bootloader at 1MB offset..."
dd if="${WORK_DIR}/u-boot.bin.sd.bin" of="${FINAL_IMAGE}" bs=512 seek=2048 ${DD_CONV}

# Step 5: Calculate checksums
echo "Calculating checksums..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    shasum -a 256 "${FINAL_IMAGE}" > "${FINAL_IMAGE}.sha256"
else
    sha256sum "${FINAL_IMAGE}" > "${FINAL_IMAGE}.sha256"
fi

# Step 6: Compress the image (optional)
echo "Compressing image..."
if command -v xz &> /dev/null; then
    echo "Creating XZ compressed image (this may take a while)..."
    xz -c -T0 "${FINAL_IMAGE}" > "${FINAL_IMAGE}.xz"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        shasum -a 256 "${FINAL_IMAGE}.xz" > "${FINAL_IMAGE}.xz.sha256"
    else
        sha256sum "${FINAL_IMAGE}.xz" > "${FINAL_IMAGE}.xz.sha256"
    fi
    echo "Compressed image created: ${FINAL_IMAGE}.xz"
fi

# Clean up
rm -rf "${WORK_DIR}"

echo ""
echo "✅ Complete image created successfully!"
echo "   Image: ${FINAL_IMAGE}"
echo "   Size: $(du -h ${FINAL_IMAGE} | cut -f1)"
echo ""
echo "To flash this image:"
echo "  1. Enter recovery mode on your Odroid Go Ultra"
echo "  2. Find your device with: diskutil list"
echo "  3. Flash with: sudo dd if=${FINAL_IMAGE} of=/dev/rdiskN bs=4m"
echo "  4. Or use Balena Etcher with ${FINAL_IMAGE}"
echo ""
echo "This is a complete image - no additional bootloader installation needed!"