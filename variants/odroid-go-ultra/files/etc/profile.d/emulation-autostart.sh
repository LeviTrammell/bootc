# Auto-start cage + ES-DE on tty2 login (only on physical console, not SSH)
# Uses a lock file to prevent restart loops if cage/ES-DE crash
if [ "$(tty)" = "/dev/tty2" ] && [ -z "$WAYLAND_DISPLAY" ]; then
    LOCKFILE="/tmp/emulation-launch.lock"
    # If launched less than 5 seconds ago, don't restart (prevents crash loop)
    if [ -f "$LOCKFILE" ]; then
        LAST=$(stat -c %Y "$LOCKFILE" 2>/dev/null || echo 0)
        NOW=$(date +%s)
        if [ $((NOW - LAST)) -lt 5 ]; then
            echo "Emulation exited too quickly, not restarting. Run emulation-launch.sh manually."
            return
        fi
    fi
    touch "$LOCKFILE"
    /usr/local/bin/emulation-launch.sh 2>$HOME/emulation.log
fi
