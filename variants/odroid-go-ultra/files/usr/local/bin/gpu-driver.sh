#!/bin/bash
# Bind-mount Mali proprietary GPU libraries over Mesa's EGL/GLES/GBM
# This keeps Mesa's libGL intact while Mali provides EGL/GLES/Vulkan
set -euo pipefail

MALI_DIR=/usr/lib64/mali
SYSTEM_DIR=/usr/lib64

# Only proceed if Mali libs are installed
if [ ! -d "$MALI_DIR" ]; then
    echo "gpu-driver: Mali libraries not found at $MALI_DIR, skipping"
    exit 0
fi

# Bind-mount Mali wrapper libraries over Mesa's versions
for lib in libEGL.so.1 libGLESv1_CM.so.1 libGLESv2.so.2 libgbm.so.1; do
    if [ -f "$MALI_DIR/$lib" ] && [ -f "$SYSTEM_DIR/$lib" ]; then
        mount --bind "$MALI_DIR/$lib" "$SYSTEM_DIR/$lib"
        echo "gpu-driver: $lib -> Mali"
    fi
done

# Update dynamic linker cache
ldconfig
