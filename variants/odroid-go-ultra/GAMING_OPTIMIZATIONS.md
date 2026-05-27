# Gaming Performance Optimizations for ODROID Go Ultra

## Context

HL2 runs via Box64 but stutters. Steam's CEF browser OOMs on 2GB RAM. The OS has no compressed swap, no gaming-tuned sysctls, and the kernel uses `-Os` (optimize for size) with default preemption. These changes bring the image in line with what gaming distros (ROCKNIX, Bazzite, CachyOS) do for performance and memory management.

## Changes

### 1. Kernel Config (`variants/odroid-go-ultra/kernel-config.fragment`)

Add a new "Gaming / latency" section:

```
# Full preemption (ROCKNIX, Bazzite, CachyOS all use this)
CONFIG_PREEMPT=y
# CONFIG_PREEMPT_VOLUNTARY is not set

# 1000Hz timer for lowest input latency
CONFIG_HZ_1000=y
# CONFIG_HZ_250 is not set
CONFIG_HZ=1000

# Transparent hugepages (madvise mode — reduces TLB misses without compaction overhead)
CONFIG_TRANSPARENT_HUGEPAGE=y
CONFIG_TRANSPARENT_HUGEPAGE_MADVISE=y

# Zram compressed swap (critical for 2GB — effectively triples usable memory)
CONFIG_ZRAM=y
CONFIG_ZRAM_BACKEND_LZ4=y
CONFIG_ZRAM_BACKEND_ZSTD=y
CONFIG_ZRAM_DEF_COMP="zstd"
CONFIG_ZRAM_WRITEBACK=y

# KSM (dedup shared pages across Box64/emulator processes)
CONFIG_KSM=y

# Scheduler
CONFIG_SCHED_AUTOGROUP=y
CONFIG_NO_HZ_IDLE=y
```

Change existing setting — switch from `-Os` to `-O2`:
```
CONFIG_CC_OPTIMIZE_FOR_PERFORMANCE=y
# CONFIG_CC_OPTIMIZE_FOR_SIZE is not set
```
**Risk**: May push kernel Image over 25MB limit. If so, revert this one change.

### 2. Kernel Cmdline (`variants/odroid-go-ultra/files/usr/local/bin/update-extlinux.sh`)

Add to `OGU_ARGS`:
```
mitigations=off
```
Single-user gaming handheld — no untrusted code. Saves measurable CPU overhead.

### 3. Zram Setup (new files)

**Install package** — add `systemd-zram-generator` to dnf install in Containerfile.

**New file**: `variants/odroid-go-ultra/files/etc/systemd/zram-generator.conf`
```ini
[zram0]
zram-size = ram
compression-algorithm = zstd
swap-priority = 100
fs-type = swap
```
With zstd ~3:1 compression on 2GB RAM, this gives ~6GB effective memory. Eliminates OOM kills from Steam/emulators.

### 4. Sysctl Gaming Tunings (new file)

**New file**: `variants/odroid-go-ultra/files/etc/sysctl.d/99-gaming.conf`
```ini
# Treat zram as RAM extension (Bazzite uses 180)
vm.swappiness=180
# No readahead for zram (no seek penalty)
vm.page-cluster=0
# Keep filesystem metadata cached
vm.vfs_cache_pressure=50
# Dirty page tuning for 2GB
vm.dirty_bytes=134217728
vm.dirty_background_bytes=33554432
# Disable proactive compaction (causes latency spikes)
vm.compaction_proactiveness=0
# Zram watermark tuning
vm.watermark_boost_factor=0
vm.watermark_scale_factor=125
# Prevent OOM
vm.min_free_kbytes=20480
# Required for Source engine / Box64 (Bazzite default)
vm.max_map_count=2147483642
# Disable NMI watchdog
kernel.nmi_watchdog=0
# Keep autogroup scheduling
kernel.sched_autogroup_enabled=1
# Reduce printk noise
kernel.printk=3 3 3 3
```

### 5. I/O Scheduler (new file)

**New file**: `variants/odroid-go-ultra/files/etc/udev/rules.d/60-io-scheduler.rules`
```
ACTION=="add|change", KERNEL=="mmcblk[0-9]*", ATTR{queue/rotational}=="0", ATTR{queue/scheduler}="bfq"
```
BFQ prioritizes latency over throughput — better for game loading from eMMC.

### 6. Box64 Binfmt Registration (new file, replaces manual systemd service)

**New file**: `variants/odroid-go-ultra/files/etc/binfmt.d/box64.conf`
```
:box64:M::\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x3e\x00:\xff\xff\xff\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff\xff\xfe\xff\xff\xff:/usr/local/bin/box64:
:box32:M::\x7fELF\x01\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x03\x00:\xff\xff\xff\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff\xff\xfe\xff\xff\xff:/usr/local/bin/box64:
```
Uses systemd-binfmt (reads `/etc/binfmt.d/` natively) instead of the manual service we created on-device.

### 7. Box64 Binary + stackfix.so (new build stage in Containerfile)

**New Containerfile build stage**: Build Box64 from source
```dockerfile
FROM --platform=linux/arm64 registry.fedoraproject.org/fedora:${FEDORA_MAJOR_VERSION} AS box64-builder
RUN dnf install -y gcc cmake make git python3 && dnf clean all
RUN git clone --depth=1 https://github.com/ptitSeb/box64.git /build/box64
WORKDIR /build/box64
RUN mkdir build && cd build && \
    cmake .. -DODROIDN2=ON -DBOX32=ON -DCMAKE_BUILD_TYPE=RelWithDebInfo && \
    make -j$(nproc)
```

**New Containerfile build stage**: Build stackfix.so
```dockerfile
# In niri-nav-builder or a separate stage:
COPY variants/odroid-go-ultra/files/usr/local/lib64/stackfix.c /tmp/stackfix.c
RUN gcc -shared -fPIC -o /build/stackfix.so /tmp/stackfix.c -ldl
```

**Main stage**:
```dockerfile
COPY --from=box64-builder /build/box64/build/box64 /usr/local/bin/box64
COPY --from=niri-nav-builder /build/stackfix.so /usr/local/lib64/stackfix.so
```

**New files**:
- `variants/odroid-go-ultra/files/usr/local/lib64/stackfix.c` — the pthread stack size shim (already exists on device)
- `variants/odroid-go-ultra/files/etc/binfmt.d/box64.conf` — binfmt registration

### 8. Box64 Default Environment (new file)

**New file**: `variants/odroid-go-ultra/files/etc/profile.d/box64-defaults.sh`
```bash
export BOX64_DYNAREC=1
export BOX64_LOG=0
```
Per-game tuning stays in `~/.box64rc` (user-managed, not baked into image).

### 9. Additional Services to Mask (Containerfile)

Add to existing `systemctl mask` line:
```
ModemManager.service fwupd.service remote-fs.target
```
Only mask services that definitely exist on Fedora 42 bootc and aren't needed.

### 10. Containerfile Changes Summary

**`variants/odroid-go-ultra/Containerfile`**:
- Add `systemd-zram-generator` to dnf install
- Add box64-builder build stage
- Add stackfix.so build in niri-nav-builder stage
- COPY box64 binary to `/usr/local/bin/box64`
- COPY stackfix.so to `/usr/local/lib64/stackfix.so`
- COPY `files/etc/systemd/zram-generator.conf`
- COPY `files/etc/sysctl.d/99-gaming.conf`
- COPY `files/etc/udev/rules.d/60-io-scheduler.rules`
- COPY `files/etc/binfmt.d/box64.conf`
- COPY `files/etc/profile.d/box64-defaults.sh`
- Add `ModemManager.service fwupd.service remote-fs.target` to mask line

**`variants/odroid-go-ultra/kernel-config.fragment`**:
- Add PREEMPT, HZ_1000, THP, zram, KSM configs
- Change CC_OPTIMIZE_FOR_SIZE to CC_OPTIMIZE_FOR_PERFORMANCE (test size)

**`variants/odroid-go-ultra/files/usr/local/bin/update-extlinux.sh`**:
- Add `mitigations=off` to OGU_ARGS

## New Files

| File | Purpose |
|------|---------|
| `files/etc/systemd/zram-generator.conf` | Zram compressed swap config |
| `files/etc/sysctl.d/99-gaming.conf` | Gaming-tuned sysctl params |
| `files/etc/udev/rules.d/60-io-scheduler.rules` | BFQ scheduler for eMMC |
| `files/etc/binfmt.d/box64.conf` | Auto-register Box64 for x86 binaries |
| `files/etc/profile.d/box64-defaults.sh` | Box64 default env vars |
| `files/usr/local/lib64/stackfix.c` | pthread stack size shim source |

## Verification

1. Build kernel: `task containers:odroid-go-ultra-kernel` — verify Image stays under 25MB
2. Build image: `task containers:odroid-go-ultra`
3. Flash and boot — verify:
   - `zramctl` shows zram0 with zstd compression
   - `sysctl vm.swappiness` returns 180
   - `cat /sys/block/mmcblk*/queue/scheduler` shows `[bfq]`
   - `cat /proc/sys/fs/binfmt_misc/box64` shows registered
   - `box64 --version` works
   - `uname -v` shows PREEMPT
   - HL2 launches and runs smoother
