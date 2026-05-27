#!/bin/bash
# Fix meson ENCL gamma table for S922X MIPI-DSI displays
#
# The meson DRM driver attempts to set the gamma LUT during early boot,
# but the ENCL hardware isn't ready yet, causing WR_RDY/ADR_RDY timeouts.
# This leaves the gamma table uninitialized, resulting in random display
# colors on each boot.
#
# By the time this service runs, the hardware is ready and we can write
# the identity gamma table (linear ramp) to get correct colors.
#
# ROCKNIX works around this by panicking on gamma failure and rebooting
# until it succeeds. This is the non-panic alternative.

exec python3 /usr/local/lib/fix-meson-gamma.py
