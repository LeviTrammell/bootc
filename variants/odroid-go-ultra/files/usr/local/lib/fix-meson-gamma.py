"""Reset the meson ENCL gamma LUT to an identity (linear) table.

The Amlogic S922X meson DRM driver fails to initialize the ENCL gamma
table during early boot (GAMMA WR_RDY/ADR_RDY timeout). By the time
userspace starts, the hardware is ready and we can program it directly
via /dev/mem.

Register reference (from drivers/gpu/drm/meson/meson_registers.h):
  L_GAMMA_CNTL_PORT = 0x1400 (byte offset 0x5000 from VPU base)
  L_GAMMA_DATA_PORT = 0x1401 (byte offset 0x5004)
  L_GAMMA_ADDR_PORT = 0x1402 (byte offset 0x5008)
"""

import mmap, os, struct, time, sys

VPU_BASE = 0xFF900000
L_GAMMA_CNTL_PORT = VPU_BASE + 0x5000
L_GAMMA_DATA_PORT = VPU_BASE + 0x5004
L_GAMMA_ADDR_PORT = VPU_BASE + 0x5008

ADR_RDY  = 1 << 5
WR_RDY   = 1 << 4
EN       = 1 << 0
AUTO_INC = 1 << 11
SEL_R    = 1 << 10
SEL_G    = 1 << 9
SEL_B    = 1 << 8

fd = os.open('/dev/mem', os.O_RDWR | os.O_SYNC)
page_base = L_GAMMA_CNTL_PORT & ~0xFFF
mm = mmap.mmap(fd, 0x1000, mmap.MAP_SHARED,
               mmap.PROT_READ | mmap.PROT_WRITE, offset=page_base)

def read_reg(offset):
    mm.seek(offset - page_base)
    return struct.unpack('<I', mm.read(4))[0]

def write_reg(offset, val):
    mm.seek(offset - page_base)
    mm.write(struct.pack('<I', val))

def wait_bit(bit, tries=1000, delay=0.001):
    for _ in range(tries):
        if read_reg(L_GAMMA_CNTL_PORT) & bit:
            return True
        time.sleep(delay)
    return False

# Identity gamma: i*4 for 10-bit range (matches kernel's default table)
gamma = [i * 4 for i in range(256)]

for name, sel in [('R', SEL_R), ('G', SEL_G), ('B', SEL_B)]:
    # Disable gamma during programming
    write_reg(L_GAMMA_CNTL_PORT, read_reg(L_GAMMA_CNTL_PORT) & ~EN)

    if not wait_bit(ADR_RDY):
        print(f'{name}: ADR_RDY timeout', file=sys.stderr)
        continue

    write_reg(L_GAMMA_ADDR_PORT, AUTO_INC | sel | 0)

    for i in range(256):
        wait_bit(WR_RDY, tries=100, delay=0.0001)
        write_reg(L_GAMMA_DATA_PORT, gamma[i])

    wait_bit(ADR_RDY)
    write_reg(L_GAMMA_ADDR_PORT, AUTO_INC | sel | 0x23)

# Re-enable gamma
write_reg(L_GAMMA_CNTL_PORT, read_reg(L_GAMMA_CNTL_PORT) | EN)

mm.close()
os.close(fd)
print('meson ENCL gamma table reset to identity')
