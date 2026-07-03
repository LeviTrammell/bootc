#!/usr/bin/env python3
"""synth-gamepad — interactive uinput device for niri-nav VM/dev testing.

Creates a virtual gamepad via /dev/uinput that advertises an 8BitDo-style
profile (BTN_SOUTH/EAST/NORTH/WEST, BTN_TL/TR/TL2/TR2, BTN_START/SELECT/MODE,
plus ABS_HAT0X/Y for the d-pad — matching what xpad reports for the
8BitDo Ultimate 2.4GHz dongle). The niri-nav HTPC profile's device
discovery picks it up via capabilities fallback when no real pad is
attached.

Reads commands from stdin, one per line:

  press <key>     — press a button (BTN_SOUTH, A, X, HOME, …)
  release <key>   — release a button
  tap <key>       — press + release with a short delay
  dpad <dir>      — emit a hat event (up/down/left/right/center)
  hold <key> <ms> — press, sleep ms, release
  combo <k1>+<k2>+<k3> <ms> — chord, held for ms (for L1+R1+Start tests)
  sleep <ms>      — pause
  quit            — exit

Names are case-insensitive. Synonyms: A↔BTN_SOUTH, B↔BTN_EAST,
X↔BTN_WEST, Y↔BTN_NORTH, L1↔BTN_TL, R1↔BTN_TR, L2↔BTN_TL2, R2↔BTN_TR2,
START↔BTN_START, SELECT/BACK↔BTN_SELECT, HOME/MODE↔BTN_MODE.

Requires the python `evdev` package: `pip install --user evdev` or
`dnf install python3-evdev`. /dev/uinput must be writable (root or
member of `input` with the uinput rule).
"""
from __future__ import annotations
import sys, time, argparse

try:
    from evdev import UInput, ecodes as e, AbsInfo
except ImportError:
    sys.stderr.write(
        "python3-evdev is required. Install with:\n"
        "  dnf install python3-evdev\n"
        "or: pip install --user evdev\n"
    )
    sys.exit(2)

# Key aliases used in the REPL. niri-nav's HTPC profile only cares about
# the evdev codes, but the alias table makes the REPL ergonomic.
ALIASES = {
    "a": e.BTN_SOUTH,    "btn_south": e.BTN_SOUTH,
    "b": e.BTN_EAST,     "btn_east":  e.BTN_EAST,
    "x": e.BTN_WEST,     "btn_west":  e.BTN_WEST,
    "y": e.BTN_NORTH,    "btn_north": e.BTN_NORTH,
    "l1": e.BTN_TL,      "lb": e.BTN_TL,      "btn_tl":  e.BTN_TL,
    "r1": e.BTN_TR,      "rb": e.BTN_TR,      "btn_tr":  e.BTN_TR,
    "l2": e.BTN_TL2,     "lt": e.BTN_TL2,     "btn_tl2": e.BTN_TL2,
    "r2": e.BTN_TR2,     "rt": e.BTN_TR2,     "btn_tr2": e.BTN_TR2,
    "l3": e.BTN_THUMBL,  "btn_thumbl": e.BTN_THUMBL,
    "r3": e.BTN_THUMBR,  "btn_thumbr": e.BTN_THUMBR,
    "start":  e.BTN_START,  "btn_start":  e.BTN_START,
    "select": e.BTN_SELECT, "back": e.BTN_SELECT, "btn_select": e.BTN_SELECT,
    "home":   e.BTN_MODE,   "guide": e.BTN_MODE,  "btn_mode": e.BTN_MODE,
}

KEYS = sorted(set(ALIASES.values()))

CAPS = {
    e.EV_KEY: KEYS,
    e.EV_ABS: [
        (e.ABS_HAT0X, AbsInfo(value=0, min=-1, max=1, fuzz=0, flat=0, resolution=0)),
        (e.ABS_HAT0Y, AbsInfo(value=0, min=-1, max=1, fuzz=0, flat=0, resolution=0)),
        # Analog sticks: same range as a real 8BitDo / Xbox 360 pad.
        (e.ABS_X,  AbsInfo(value=0, min=-32768, max=32767, fuzz=16, flat=128, resolution=0)),
        (e.ABS_Y,  AbsInfo(value=0, min=-32768, max=32767, fuzz=16, flat=128, resolution=0)),
        (e.ABS_RX, AbsInfo(value=0, min=-32768, max=32767, fuzz=16, flat=128, resolution=0)),
        (e.ABS_RY, AbsInfo(value=0, min=-32768, max=32767, fuzz=16, flat=128, resolution=0)),
    ],
}


def resolve(name: str) -> int:
    key = ALIASES.get(name.lower())
    if key is None:
        raise KeyError(f"unknown button: {name}")
    return key


def emit_key(ui: UInput, code: int, pressed: bool) -> None:
    ui.write(e.EV_KEY, code, 1 if pressed else 0)
    ui.syn()


def emit_stick(ui: UInput, side: str, x: int, y: int) -> None:
    """Set the absolute axis values for left ('l') or right ('r') stick.

    x/y in [-32768, 32767]. Range checks are kernel-side; we just write.
    """
    side = side.lower()
    if side in ("l", "left"):
        ui.write(e.EV_ABS, e.ABS_X, x)
        ui.write(e.EV_ABS, e.ABS_Y, y)
    elif side in ("r", "right"):
        ui.write(e.EV_ABS, e.ABS_RX, x)
        ui.write(e.EV_ABS, e.ABS_RY, y)
    else:
        raise KeyError(f"unknown stick side: {side}")
    ui.syn()


def emit_hat(ui: UInput, direction: str) -> None:
    direction = direction.lower()
    if direction in ("up",):
        ui.write(e.EV_ABS, e.ABS_HAT0Y, -1)
    elif direction in ("down",):
        ui.write(e.EV_ABS, e.ABS_HAT0Y, 1)
    elif direction in ("left",):
        ui.write(e.EV_ABS, e.ABS_HAT0X, -1)
    elif direction in ("right",):
        ui.write(e.EV_ABS, e.ABS_HAT0X, 1)
    elif direction in ("center", "c"):
        ui.write(e.EV_ABS, e.ABS_HAT0X, 0)
        ui.write(e.EV_ABS, e.ABS_HAT0Y, 0)
    else:
        raise KeyError(f"unknown direction: {direction}")
    ui.syn()


def handle(ui: UInput, line: str) -> bool:
    """Return False to stop the loop, True to continue."""
    tokens = line.strip().split()
    if not tokens or tokens[0].startswith("#"):
        return True
    cmd = tokens[0].lower()
    args = tokens[1:]

    if cmd == "quit":
        return False
    if cmd == "sleep":
        time.sleep(int(args[0]) / 1000.0)
        return True
    if cmd == "press":
        emit_key(ui, resolve(args[0]), True)
        return True
    if cmd == "release":
        emit_key(ui, resolve(args[0]), False)
        return True
    if cmd == "tap":
        code = resolve(args[0])
        emit_key(ui, code, True)
        time.sleep(0.04)
        emit_key(ui, code, False)
        return True
    if cmd == "hold":
        code = resolve(args[0])
        ms = int(args[1])
        emit_key(ui, code, True)
        time.sleep(ms / 1000.0)
        emit_key(ui, code, False)
        return True
    if cmd == "combo":
        if "+" not in args[0]:
            raise ValueError("combo expects '<k1>+<k2>...' as first arg")
        codes = [resolve(name) for name in args[0].split("+")]
        ms = int(args[1])
        for c in codes:
            emit_key(ui, c, True)
        time.sleep(ms / 1000.0)
        for c in reversed(codes):
            emit_key(ui, c, False)
        return True
    if cmd == "dpad":
        emit_hat(ui, args[0])
        return True
    if cmd == "stick":
        # stick l 12000 -8000   — deflect left stick to (x, y)
        # stick l 0 0           — return to centre
        emit_stick(ui, args[0], int(args[1]), int(args[2]))
        return True

    sys.stderr.write(f"unknown command: {cmd}\n")
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--name",
        default="8BitDo Ultimate Wireless Controller",
        help="Device name (default matches what niri-nav looks for)",
    )
    parser.add_argument(
        "--vendor", type=lambda s: int(s, 0), default=0x2DC8,
        help="USB vendor id (default 0x2dc8 = 8BitDo)",
    )
    parser.add_argument(
        "--product", type=lambda s: int(s, 0), default=0x3109,
        help="USB product id (default 0x3109 = Ultimate 2.4GHz)",
    )
    args = parser.parse_args()

    ui = UInput(CAPS, name=args.name, vendor=args.vendor, product=args.product)
    print(f"synth-gamepad: device created as {args.name}", file=sys.stderr)
    print("type 'quit' or Ctrl-D to exit", file=sys.stderr)

    try:
        for line in sys.stdin:
            try:
                if not handle(ui, line):
                    break
            except (KeyError, ValueError, IndexError) as ex:
                sys.stderr.write(f"error: {ex}\n")
    finally:
        ui.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
