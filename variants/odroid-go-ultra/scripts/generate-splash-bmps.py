"""Generate retro-themed splash BMPs for ODROID Go Ultra U-Boot.

Creates 854x480 24-bit BMPs with a green-on-black terminal aesthetic.
"""

from PIL import Image, ImageDraw, ImageFont
import os

W, H = 854, 480
BG = (0, 0, 0)
GREEN = (0, 204, 0)
DIM_GREEN = (0, 85, 0)
BRIGHT_GREEN = (0, 255, 0)
DARK_GREEN = (0, 40, 0)
RED = (204, 68, 68)
YELLOW = (204, 204, 68)

OUT = os.path.join(os.path.dirname(__file__),
                   '..', 'files', 'boot', 'res')
os.makedirs(OUT, exist_ok=True)


def new_img():
    return Image.new('RGB', (W, H), BG)


def draw_scanlines(draw, opacity=0.15):
    """Subtle horizontal scanline effect."""
    for y in range(0, H, 3):
        draw.line([(0, y), (W - 1, y)], fill=DARK_GREEN, width=1)


def draw_border(draw, color=DIM_GREEN, margin=8, thickness=2):
    """Retro terminal border."""
    for i in range(thickness):
        m = margin + i
        draw.rectangle([(m, m), (W - 1 - m, H - 1 - m)], outline=color)


def draw_corner_marks(draw, color=DIM_GREEN, margin=8, size=20):
    """Corner bracket marks."""
    m = margin
    s = size
    # Top-left
    draw.line([(m, m + s), (m, m), (m + s, m)], fill=color, width=2)
    # Top-right
    draw.line([(W - m - s, m), (W - m, m), (W - m, m + s)], fill=color, width=2)
    # Bottom-left
    draw.line([(m, H - m - s), (m, H - m), (m + s, H - m)], fill=color, width=2)
    # Bottom-right
    draw.line([(W - m - s, H - m), (W - m, H - m), (W - m, H - m - s)],
              fill=color, width=2)


def large_text(draw, text, y, color=GREEN, scale=5):
    """Draw blocky pixel-art text centered at y.

    Uses a simple 5x7 pixel font scaled up.
    """
    glyphs = {
        'O': ['01110', '10001', '10001', '10001', '10001', '10001', '01110'],
        'D': ['11100', '10010', '10001', '10001', '10001', '10010', '11100'],
        'R': ['11110', '10001', '10001', '11110', '10100', '10010', '10001'],
        'I': ['11111', '00100', '00100', '00100', '00100', '00100', '11111'],
        'G': ['01110', '10001', '10000', '10111', '10001', '10001', '01110'],
        ' ': ['00000', '00000', '00000', '00000', '00000', '00000', '00000'],
        'U': ['10001', '10001', '10001', '10001', '10001', '10001', '01110'],
        'L': ['10000', '10000', '10000', '10000', '10000', '10000', '11111'],
        'T': ['11111', '00100', '00100', '00100', '00100', '00100', '00100'],
        'A': ['01110', '10001', '10001', '11111', '10001', '10001', '10001'],
        'S': ['01111', '10000', '10000', '01110', '00001', '00001', '11110'],
        'Y': ['10001', '10001', '01010', '00100', '00100', '00100', '00100'],
        'E': ['11111', '10000', '10000', '11110', '10000', '10000', '11111'],
        'M': ['10001', '11011', '10101', '10101', '10001', '10001', '10001'],
        'W': ['10001', '10001', '10001', '10101', '10101', '11011', '10001'],
        'B': ['11110', '10001', '10001', '11110', '10001', '10001', '11110'],
        'C': ['01110', '10001', '10000', '10000', '10000', '10001', '01110'],
        'F': ['11111', '10000', '10000', '11110', '10000', '10000', '10000'],
        'H': ['10001', '10001', '10001', '11111', '10001', '10001', '10001'],
        'N': ['10001', '11001', '10101', '10011', '10001', '10001', '10001'],
        'P': ['11110', '10001', '10001', '11110', '10000', '10000', '10000'],
        'V': ['10001', '10001', '10001', '10001', '01010', '01010', '00100'],
        'X': ['10001', '01010', '00100', '00100', '00100', '01010', '10001'],
        'K': ['10001', '10010', '10100', '11000', '10100', '10010', '10001'],
        '0': ['01110', '10011', '10101', '10101', '10101', '11001', '01110'],
        '1': ['00100', '01100', '00100', '00100', '00100', '00100', '01110'],
        '2': ['01110', '10001', '00001', '00110', '01000', '10000', '11111'],
        '3': ['01110', '10001', '00001', '00110', '00001', '10001', '01110'],
        '4': ['00010', '00110', '01010', '10010', '11111', '00010', '00010'],
        '%': ['11001', '11010', '00100', '00100', '01011', '10011', '00011'],
        '.': ['00000', '00000', '00000', '00000', '00000', '01100', '01100'],
        '!': ['00100', '00100', '00100', '00100', '00100', '00000', '00100'],
        '-': ['00000', '00000', '00000', '11111', '00000', '00000', '00000'],
        ':': ['00000', '01100', '01100', '00000', '01100', '01100', '00000'],
    }
    char_w = 5 * scale + scale  # char width + spacing
    total_w = len(text) * char_w - scale
    start_x = (W - total_w) // 2

    for ci, ch in enumerate(text):
        glyph = glyphs.get(ch.upper(), glyphs.get(' '))
        if not glyph:
            continue
        for row_i, row in enumerate(glyph):
            for col_i, pixel in enumerate(row):
                if pixel == '1':
                    x = start_x + ci * char_w + col_i * scale
                    yy = y + row_i * scale
                    draw.rectangle([(x, yy), (x + scale - 1, yy + scale - 1)],
                                   fill=color)


def draw_battery(draw, x, y, w, h, fill_pct, color=GREEN):
    """Draw a battery icon."""
    # Battery body
    draw.rectangle([(x, y), (x + w, y + h)], outline=color, width=2)
    # Battery tip
    tip_w = w // 6
    tip_h = h // 3
    draw.rectangle([(x + w, y + h // 3), (x + w + tip_w, y + 2 * h // 3)],
                   fill=color)
    # Fill level
    if fill_pct > 0:
        fill_w = int((w - 6) * fill_pct)
        fill_color = RED if fill_pct < 0.15 else (YELLOW if fill_pct < 0.30 else color)
        draw.rectangle([(x + 3, y + 3), (x + 3 + fill_w, y + h - 3)],
                       fill=fill_color)


# ─── logo.bmp ───
img = new_img()
draw = ImageDraw.Draw(img)
draw_scanlines(draw)
draw_corner_marks(draw, BRIGHT_GREEN, margin=12, size=40)

# Main title
large_text(draw, 'ODROID', H // 2 - 60, BRIGHT_GREEN, scale=6)
large_text(draw, 'GO ULTRA', H // 2 + 10, GREEN, scale=5)

# Subtitle line
large_text(draw, 'SYSTEM BOOT', H // 2 + 80, DIM_GREEN, scale=3)

# Decorative lines
draw.line([(80, H // 2 - 75), (W - 80, H // 2 - 75)], fill=DIM_GREEN, width=1)
draw.line([(80, H // 2 + 70), (W - 80, H // 2 + 70)], fill=DIM_GREEN, width=1)

img.save(os.path.join(OUT, 'logo.bmp'), 'BMP')
print('logo.bmp')

# ─── Battery BMPs ───
for level, name in [(0, 'batt_0'), (1, 'batt_1'), (2, 'batt_2'), (3, 'batt_3')]:
    img = new_img()
    draw = ImageDraw.Draw(img)
    draw_scanlines(draw)
    draw_corner_marks(draw)
    pct = level / 3.0
    bw, bh = 200, 100
    bx = (W - bw) // 2
    by = H // 2 - 80
    draw_battery(draw, bx, by, bw, bh, pct)
    pct_text = f'{int(pct * 100)}%'
    large_text(draw, pct_text, by + bh + 30, GREEN, scale=4)
    img.save(os.path.join(OUT, f'{name}.bmp'), 'BMP')
    print(f'{name}.bmp')

# ─── batt_low.bmp ───
img = new_img()
draw = ImageDraw.Draw(img)
draw_scanlines(draw)
draw_corner_marks(draw, RED)
bw, bh = 200, 100
bx, by = (W - bw) // 2, H // 2 - 80
draw_battery(draw, bx, by, bw, bh, 0.05, RED)
large_text(draw, 'LOW BATTERY', by + bh + 30, RED, scale=4)
img.save(os.path.join(OUT, 'batt_low.bmp'), 'BMP')
print('batt_low.bmp')

# ─── batt_fail.bmp ───
img = new_img()
draw = ImageDraw.Draw(img)
draw_scanlines(draw)
draw_corner_marks(draw, RED)
bw, bh = 200, 100
bx, by = (W - bw) // 2, H // 2 - 80
draw_battery(draw, bx, by, bw, bh, 0.0, RED)
# X over battery
draw.line([(bx, by), (bx + bw, by + bh)], fill=RED, width=3)
draw.line([(bx + bw, by), (bx, by + bh)], fill=RED, width=3)
large_text(draw, 'BATT FAIL', by + bh + 30, RED, scale=4)
img.save(os.path.join(OUT, 'batt_fail.bmp'), 'BMP')
print('batt_fail.bmp')

# ─── recovery.bmp ───
img = new_img()
draw = ImageDraw.Draw(img)
draw_scanlines(draw)
draw_corner_marks(draw, YELLOW)
large_text(draw, 'RECOVERY', H // 2 - 30, YELLOW, scale=6)
large_text(draw, 'MODE', H // 2 + 40, YELLOW, scale=5)
img.save(os.path.join(OUT, 'recovery.bmp'), 'BMP')
print('recovery.bmp')

# ─── sys_err.bmp ───
img = new_img()
draw = ImageDraw.Draw(img)
draw_scanlines(draw)
draw_corner_marks(draw, RED)
large_text(draw, 'SYSTEM', H // 2 - 30, RED, scale=6)
large_text(draw, 'ERROR', H // 2 + 40, RED, scale=5)
img.save(os.path.join(OUT, 'sys_err.bmp'), 'BMP')
print('sys_err.bmp')

# ─── auto_test.bmp ───
img = new_img()
draw = ImageDraw.Draw(img)
draw_scanlines(draw)
draw_corner_marks(draw)
large_text(draw, 'AUTO TEST', H // 2 - 20, GREEN, scale=5)
img.save(os.path.join(OUT, 'auto_test.bmp'), 'BMP')
print('auto_test.bmp')

print(f'\nAll BMPs saved to {os.path.abspath(OUT)}')
