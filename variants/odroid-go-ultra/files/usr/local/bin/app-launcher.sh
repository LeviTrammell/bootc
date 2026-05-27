#!/bin/bash
# Simple app launcher for ODROID Go Ultra
# Shows .desktop apps in fzf, launches selection through niri
choice=$(
    for f in /usr/share/applications/*.desktop; do
        grep -q "NoDisplay=true" "$f" 2>/dev/null && continue
        name=$(grep "^Name=" "$f" | head -1 | cut -d= -f2)
        exec=$(grep "^Exec=" "$f" | head -1 | cut -d= -f2 | sed 's/ %[fFuUdDnNickvm]//g')
        [ -n "$name" ] && [ -n "$exec" ] && printf '%s|%s\n' "$exec" "$name"
    done | sort -t'|' -k2 |
    fzf --delimiter='|' --with-nth=2 --no-info --reverse --no-mouse \
        --color=bg+:#3b4252,fg+:#88c0d0,hl:#ebcb8b,hl+:#ebcb8b \
        --prompt='> ' --pointer='▸'
)

[ -n "$choice" ] && exec niri msg action spawn -- "${choice%%|*}"
