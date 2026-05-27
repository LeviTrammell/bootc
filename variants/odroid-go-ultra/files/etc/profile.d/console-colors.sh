# Green-on-black palette for physical console (retro CRT look)
case "$(tty)" in
  /dev/tty[0-9]*)
    printf '\e]P0000000'  # 0: black
    printf '\e]P1882222'  # 1: dark red
    printf '\e]P200aa00'  # 2: green
    printf '\e]P3888822'  # 3: olive
    printf '\e]P4224488'  # 4: dark blue
    printf '\e]P5882288'  # 5: purple
    printf '\e]P6228888'  # 6: teal
    printf '\e]P700cc00'  # 7: green (default fg)
    printf '\e]P8005500'  # 8: dim green
    printf '\e]P9cc4444'  # 9: red
    printf '\e]PA00ff00'  # A: bright green
    printf '\e]PBcccc44'  # B: yellow
    printf '\e]PC4488cc'  # C: blue
    printf '\e]PDcc44cc'  # D: magenta
    printf '\e]PE44cccc'  # E: cyan
    printf '\e]PF00ff00'  # F: bright green (white)
    clear
    ;;
esac
