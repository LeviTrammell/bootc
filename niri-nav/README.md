# niri-nav

Gamepad-driven navigation daemon for niri-based bootc variants.

The daemon runs as a systemd user service. It grabs the gamepad evdev
device and translates button presses into niri IPC calls (focus, move
window, etc.), wtype key injection, forwarded intent commands to the
controller-shell overlay, or shell-outs to per-variant control scripts.
A universal `L1+R1+Start` hold drops out of "nav" into "passthrough" so
the foreground app gets raw gamepad input.

## controller-shell integration

The shell (../controller-shell) is the on-screen home: mode tiles + app
launcher, rendered as a gtk4-layer-shell overlay. niri-nav owns the
gamepad and forwards intent (`up`, `select`, `back`, ...) over
`$XDG_RUNTIME_DIR/controller-shell.sock`; the shell answers back on
`$XDG_RUNTIME_DIR/niri-nav.sock` with `{"cmd":"mode","name":"..."}`
when the user picks a tile (wire types in ../cs-proto). Which tiles
exist and what they do is per-variant config —
`/etc/controller-shell/config.json`.

On the OGU the flow is console-like: boot lands in `Shell` mode with
the overlay visible; the Games tile enters `Emulation` mode (chvt 2 →
ES-DE on TTY2); a VT watcher notices the console returning to TTY1 and
drops back to `WindowNav`. `Start` summons the shell from anywhere in
niri.

The Browser tile spawns Zen and enters `Browser` mode: left stick =
cursor (joystick-cursor), right stick = scroll, d-pad Down/Up =
next/prev interactable element (Tab / Shift+Tab, PSP-style), A =
activate focused element, X/R2 = click at cursor, L2 = right click,
B = back, L1/R1 = prev/next tab, Select = URL bar, F3+A/X = new/close
tab, F3+B = forward, F3+Y = reload. The OSK is PSP-style: osk-overlay
is an input-method-v2 client, so focusing any text field (d-pad hop or
click) auto-shows the keyboard and flips niri-nav into `TextEntry`
(blur reverses it); in `TextEntry`, `Select` cycles lower/UPPER/symbol
layers and `Start` sends Return.

## Build

Exactly one profile feature must be enabled.

```
# OGU (Odroid Go Ultra, handheld) — default
cargo build --release

# HTPC (Odroid H2+ / 8BitDo Ultimate)
cargo build --release --no-default-features --features htpc
```

Both produce `target/release/niri-nav`.

## Architecture

```
src/
  button.rs       — logical Button enum (shared vocabulary)
  modifiers.rs    — shared modifier state (L1/R1)
  input.rs        — grab/ungrab helpers (shared)
  ipc_server.rs   — niri-nav.sock listener (mode commands from the shell)
  profile.rs      — ControllerProfile trait
  runtime.rs      — profile-agnostic event loop
  profiles/
    ogu/          — Odroid Go Ultra handheld
      niri.rs     — niri IPC actions
      wtype.rs    — wayland key injection
      ...         — merger/cursor/OSK child-process managers
    htpc/         — Odroid H2+ HTPC (8BitDo)
```

A profile owns:

- device probing (which evdev device is the gamepad)
- raw evdev → logical Button mapping
- the set of modes (per-profile enum)
- per-mode dispatch (what each button does)
- which mode is "passthrough"
- optional extra combo (OGU has L1+R1+F6 → TTY2 emulation)
- mode-transition/startup hooks (spawn/kill overlays, pause helper daemons)
- an idle tick hook (OGU watches the active VT there)

The runtime owns the event loop, key repeat, the universal L1+R1+Start
combo, grab/ungrab, mode persistence across restarts, the IPC server,
and shared modifier tracking.

## Dev loop

Iteration outside the target hardware uses a synthetic uinput gamepad.
You only need `python3-evdev` and write access to `/dev/uinput`. Three
options, fastest first:

### Container

Spin up a Fedora container with `/dev/uinput` and the host's
`/dev/input` bind-mounted. The host's udev is what creates evdev nodes
in `/dev/input`, so a bind-mount is required for the synthetic pad
created inside the container to be visible. niri does not need to be
running — `niri msg` calls log a warning and the event loop stays
exercisable. Source is mounted in for live edits.

```
sudo modprobe uinput  # one-time
sudo podman run --rm -it --privileged \
    -v /dev/input:/dev/input \
    -v "$PWD/niri-nav:/work:Z" \
    -w /work \
    registry.fedoraproject.org/fedora:43 \
    bash -lc '
        dnf install -y rust cargo python3-evdev
        cargo build --no-default-features --features htpc
        # In one shell: run niri-nav
        RUST_LOG=info ./target/debug/niri-nav
    '
```

In another terminal (also `sudo podman exec` into the same container or
into a sibling container with the same mounts), drive the synth pad:

```
python3 scripts/synth-gamepad.py <<EOF
press home
sleep 100
release home
combo l1+r1+start 600
quit
EOF
```

Verified output for the above script:

```
Selected gamepad: 8BitDo Ultimate Wireless Controller (/dev/input/eventN)
Device grabbed
Mode: DASHBOARD
spawn picker
Mode: DASHBOARD -> PICKER
Passthrough combo fired
Mode: PICKER -> PASSTHROUGH
Device ungrabbed
```

`--privileged` is needed because SELinux blocks evdev reads on the
bind-mounted host device nodes from a confined container. For a
hardened dev container, label policy could be tuned instead — but for
local iteration `--privileged` is fine.

### Workstation-direct

Run niri-nav against a synthetic pad without spinning up a VM or container:

```
sudo dnf install python3-evdev
cargo build --no-default-features --features htpc
WAYLAND_DISPLAY=$WAYLAND_DISPLAY RUST_LOG=debug \
    ./target/debug/niri-nav &

# In another shell:
sudo ./scripts/synth-gamepad.py <<'EOF'
press home
sleep 100
release home
EOF
```

`niri-nav` will pick up the synthetic device by capabilities (BTN_SOUTH +
BTN_MODE) and log the event. niri itself doesn't need to be running for
the daemon to receive events — most dispatch side effects shell out to
`niri msg action`, which will just fail with a warning if niri isn't
running, leaving the event-loop behavior testable.

### QEMU VM

For full integration (niri running, htpc-ctl on PATH, mode-picker
spawn), build the HTPC bootc image and boot it in QEMU. The synthetic
pad runs inside the guest:

```
task images:htpc                        # build dist/htpc/disk.raw (or qcow2)
qemu-system-x86_64 \
    -enable-kvm -m 4G -smp 2 -cpu host \
    -drive file=dist/htpc/disk.raw,format=raw,if=virtio \
    -nic user,model=virtio-net-pci,hostfwd=tcp::2222-:22 \
    -display gtk -vga virtio

ssh -p 2222 levi@localhost
# inside guest:
sudo dnf install python3-evdev
sudo ./synth-gamepad.py < script.txt
```

## REPL commands (synth-gamepad)

```
press <key>            press button (e.g. 'press home')
release <key>          release button
tap <key>              press + brief release
hold <key> <ms>        press, sleep, release
combo <k1>+<k2>+... <ms>   chord held for ms
dpad <up|down|left|right|center>   hat event
sleep <ms>             pause
quit                   exit
```

Aliases: A↔BTN_SOUTH, B↔BTN_EAST, X↔BTN_WEST, Y↔BTN_NORTH, L1↔BTN_TL,
R1↔BTN_TR, START↔BTN_START, BACK/SELECT↔BTN_SELECT, HOME/MODE↔BTN_MODE.

## Adding a new profile

1. `src/profiles/<name>/{mod.rs, button.rs, input.rs, state.rs}`
2. Implement `ControllerProfile` for your profile struct.
3. Add `<name> = []` to `Cargo.toml` features.
4. Add the cfg arms to `src/profiles/mod.rs`.
