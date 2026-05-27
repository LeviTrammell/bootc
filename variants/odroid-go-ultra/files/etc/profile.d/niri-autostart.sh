# Auto-start niri on tty1 login (only on physical console, not SSH)
if [ "$(tty)" = "/dev/tty1" ] && [ -z "$WAYLAND_DISPLAY" ]; then
    exec niri --session 2>$HOME/niri.log
fi
