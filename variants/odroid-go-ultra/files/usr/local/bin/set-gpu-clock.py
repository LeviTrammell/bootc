#!/usr/bin/env python3
"""Set Amlogic S922X GPU (Mali-G52) clock via HHI_MALI_CLK_CNTL register.

The GPU defaults to 24MHz (crystal oscillator) without DVFS. This script
switches to fclk_div2p5 (800MHz) using the mali_1 clock mux to avoid glitches.

Clock sources: 0=xtal(24M), 1=gp0_pll, 2=hifi_pll, 3=fclk_div2p5(800M),
               4=fclk_div3(667M), 5=fclk_div4(500M), 6=fclk_div5(400M), 7=fclk_div7(286M)
"""
import mmap, struct, os, time

HHI_BASE = 0xFF63C000
MALI_CLK_OFFSET = 0x1B0
TARGET_SOURCE = 3  # fclk_div2p5 = 800MHz
TARGET_DIV = 0     # divide by 1

fd = os.open("/dev/mem", os.O_RDWR | os.O_SYNC)
m = mmap.mmap(fd, 4096, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE, offset=HHI_BASE)

old = struct.unpack("<I", m[MALI_CLK_OFFSET:MALI_CLK_OFFSET+4])[0]

# Configure mali_1 with target clock, keep mali_0 running, mux on mali_0
val = (0 << 31) | (TARGET_SOURCE << 25) | (1 << 24) | (TARGET_DIV << 16) | (old & 0xFFFF)
m[MALI_CLK_OFFSET:MALI_CLK_OFFSET+4] = struct.pack("<I", val)
time.sleep(0.001)

# Switch final mux to mali_1
val |= (1 << 31)
m[MALI_CLK_OFFSET:MALI_CLK_OFFSET+4] = struct.pack("<I", val)

m.close()
os.close(fd)
print(f"GPU clock set to 800MHz (reg 0x{old:08X} -> 0x{val:08X})")
