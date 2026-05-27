#!/bin/bash
# Emulation launcher: starts cage (Wayland kiosk) with ES-DE on TTY2.
# Also starts vt-switch-monitor for L1+R1+F6 -> TTY1 switching,
# and gamepad-merger so emulators see a proper virtual gamepad.

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

# Create ROM directories if they don't exist
mkdir -p ~/ROMs/{ps2,psx,n64,snes,gba,megadrive,nes,arcade,gb,gbc}
mkdir -p ~/.config/aethersx2/bios
mkdir -p ~/.config/retroarch/system

# Start vt-switch-monitor in background (L1+R1+F6 -> chvt 1)
/usr/local/bin/vt-switch-monitor &
VT_MON_PID=$!

# Start gamepad-merger in always-active mode so emulators see a proper gamepad
# (merges adc-joystick-left, adc-joystick-right, gpio-keys into one virtual device)
(echo "ACTIVATE"; sleep infinity) | /usr/local/bin/gamepad-merger &
MERGER_PID=$!
sleep 0.5  # let virtual device appear

# SDL2 GameController mapping for the OGU Virtual Gamepad
export SDL_GAMECONTROLLERCONFIG="0600d4ca474f00003055000001000000,OGU Virtual Gamepad,platform:Linux,a:b0,b:b1,x:b3,y:b2,leftshoulder:b4,rightshoulder:b5,lefttrigger:b6,righttrigger:b7,back:b8,start:b9,dpup:b10,dpdown:b11,dpleft:b12,dpright:b13,leftx:a0,lefty:a1,rightx:a2,righty:a3,"

cage -s -- bash -c '
    wlr-randr --output DSI-1 --transform 90 2>/dev/null
    exec es-de
'

# When cage exits, clean up
kill $VT_MON_PID 2>/dev/null
kill $MERGER_PID 2>/dev/null
