"""Generates assets/retype.ico and assets/retype.png: "re" plus a text cursor.

Standard library only, deterministic, so the committed icon can be rebuilt
and checked: python tools/make_icon.py

Shapes are defined on a 64x64 canvas and rasterised with supersampling.
"""

import math
import struct
import zlib
from pathlib import Path

BACKGROUND = (0x2C, 0x2C, 0x2A)
TEXT = (0xF1, 0xEF, 0xE8)
CURSOR = (0xEF, 0x9F, 0x27)

ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 256]
PNG_SIZES = {256}  # stored as PNG inside the .ico (Vista+); others as BMP

ROOT = Path(__file__).resolve().parent.parent


def rounded_square(x, y):
    left, top, right, bottom, radius = 2.0, 2.0, 62.0, 62.0, 14.0
    if not (left <= x <= right and top <= y <= bottom):
        return False
    dx = max(left + radius - x, x - (right - radius), 0.0)
    dy = max(top + radius - y, y - (bottom - radius), 0.0)
    return dx * dx + dy * dy <= radius * radius


# Letters are strokes of one width with round ends, like a geometric font.
STROKE = 5.5
HALF = STROKE / 2


def rect(x, y, x0, x1, y0, y1):
    return x0 <= x <= x1 and y0 <= y <= y1


def near_segment(x, y, ax, ay, bx, by):
    """Within half a stroke of the segment a-b (round caps)."""
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(x - (ax + t * dx), y - (ay + t * dy)) <= HALF


def near_arc(x, y, cx, cy, radius, start, end):
    """Within half a stroke of the arc from angle `start` to `end` (radians,
    y pointing down, so angles grow clockwise on screen), with round caps."""
    angle = math.atan2(y - cy, x - cx) % (2 * math.pi)
    span = (end - start) % (2 * math.pi) or 2 * math.pi
    if (angle - start) % (2 * math.pi) <= span:
        return abs(math.hypot(x - cx, y - cy) - radius) <= HALF
    ends = [(cx + radius * math.cos(a), cy + radius * math.sin(a)) for a in (start, end)]
    return any(math.hypot(x - ex, y - ey) <= HALF for ex, ey in ends)


def letter_r(x, y):
    stem = near_segment(x, y, 10.5, 23.5, 10.5, 41.5)
    # Shoulder: from the stem over the top to the upper right.
    shoulder = near_arc(x, y, 18.5, 31.5, 8.0, math.pi, 1.75 * math.pi)
    return stem or shoulder


def letter_e(x, y):
    cx, cy, radius = 37.5, 32.5, 9.0
    # Bowl: from the lower-right terminal clockwise all the way to the bar.
    bowl = near_arc(x, y, cx, cy, radius, 0.28 * math.pi, 2 * math.pi)
    bar = near_segment(x, y, cx - radius, cy, cx + radius, cy)
    return bowl or bar


def cursor(x, y):
    # A bar with slightly rounded ends, taller than the letters.
    return near_segment_width(x, y, 53.75, 15.5, 53.75, 48.5, 2.25)


def near_segment_width(x, y, ax, ay, bx, by, half):
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(x - (ax + t * dx), y - (ay + t * dy)) <= half


LAYERS = [
    (rounded_square, BACKGROUND),
    (lambda x, y: letter_r(x, y) or letter_e(x, y), TEXT),
    (cursor, CURSOR),
]


def render(size):
    """Straight-alpha RGBA rows, top to bottom."""
    samples = 8 if size <= 32 else 4
    scale = 64.0 / size
    rows = []
    for py in range(size):
        row = []
        for px in range(size):
            r = g = b = a = 0.0
            for sy in range(samples):
                for sx in range(samples):
                    x = (px + (sx + 0.5) / samples) * scale
                    y = (py + (sy + 0.5) / samples) * scale
                    color = None
                    for inside, layer_color in LAYERS:
                        if inside(x, y):
                            color = layer_color
                    if color is not None and rounded_square(x, y):
                        r += color[0]
                        g += color[1]
                        b += color[2]
                        a += 1
            n = samples * samples
            if a:
                row.append((round(r / a), round(g / a), round(b / a), round(255 * a / n)))
            else:
                row.append((0, 0, 0, 0))
        rows.append(row)
    return rows


def png(rows):
    size = len(rows)
    raw = b"".join(b"\x00" + bytes(c for px in row for c in px) for row in rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def bmp(rows):
    """ICO-style DIB: BITMAPINFOHEADER (double height), BGRA bottom-up, AND mask."""
    size = len(rows)
    header = struct.pack("<IiiHHIIiiII", 40, size, size * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = b"".join(
        bytes(c for (r, g, b, a) in row for c in (b, g, r, a)) for row in reversed(rows)
    )
    mask_row = ((size + 31) // 32) * 4
    mask = b"\x00" * (mask_row * size)  # alpha decides; the AND mask stays clear
    return header + pixels + mask


def ico(images):
    entries = []
    offset = 6 + 16 * len(images)
    for size, data in images:
        dim = 0 if size >= 256 else size
        entries.append(struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset))
        offset += len(data)
    return (
        struct.pack("<HHH", 0, 1, len(images))
        + b"".join(entries)
        + b"".join(data for _, data in images)
    )


def main():
    images = []
    for size in ICO_SIZES:
        rows = render(size)
        images.append((size, png(rows) if size in PNG_SIZES else bmp(rows)))
    assets = ROOT / "assets"
    assets.mkdir(exist_ok=True)
    (assets / "retype.ico").write_bytes(ico(images))
    (assets / "retype.png").write_bytes(png(render(256)))
    print("wrote", assets / "retype.ico", "and", assets / "retype.png")


if __name__ == "__main__":
    main()
