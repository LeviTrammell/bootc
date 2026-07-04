# Persistent TTY2 emulation session (runs in the autologin shell).
#
# cage's wlroots backend aborts after 10s if its VT isn't active, so the
# session must only ever be launched while TTY2 is the active console.
# This loop waits for TTY2 activation, runs the emulation stack, drops
# back to TTY1 when it exits (ES-DE quit or crash), and re-arms —
# niri-nav's VT watcher notices the return to TTY1 and re-grabs the pad.
if [ "$(tty)" = "/dev/tty2" ] && [ -z "$WAYLAND_DISPLAY" ]; then
    while true; do
        while [ "$(cat /sys/class/tty/tty0/active)" != "tty2" ]; do
            sleep 0.5
        done
        START=$(date +%s)
        /usr/local/bin/emulation-launch.sh >>"$HOME/emulation.log" 2>&1
        sudo chvt 1
        # Rapid-exit guard: if the stack died within 5s and we somehow
        # remain on TTY2, wait for a VT cycle instead of crash-looping.
        if [ $(( $(date +%s) - START )) -lt 5 ]; then
            echo "emulation session exited too quickly; waiting for VT cycle" >>"$HOME/emulation.log"
            while [ "$(cat /sys/class/tty/tty0/active)" = "tty2" ]; do
                sleep 1
            done
        fi
    done
fi
