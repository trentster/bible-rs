#!/usr/bin/env python3
"""Draws the Lamplit icon placeholder and three rough directions, for Mark to choose from.

    python3 scripts/draft-lamplit-icons.py

Writes:
    packaging/logo/lamplit-placeholder.png   1024 px, what ships until Mark approves art
    /tmp/lamplit-draft-a.png                 512 px, "Lamp, plain"
    /tmp/lamplit-draft-b.png                 512 px, "Lamp at night"
    /tmp/lamplit-draft-c.png                 512 px, "Page and wick"

These are geometry, not artwork: enough to choose a direction and nothing more.
"""

import math
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
PAPER = (250, 247, 241, 255)
INK = (28, 29, 33, 255)
ACCENT = (45, 74, 158, 255)
WARM = (224, 166, 75, 255)
NIGHT = (25, 28, 36, 255)


def canvas(size: int, colour) -> Image.Image:
    return Image.new("RGBA", (size, size), colour)


def rounded_mask(size: int, radius_ratio: float = 0.22) -> Image.Image:
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    r = int(size * radius_ratio)
    draw.rounded_rectangle([0, 0, size - 1, size - 1], radius=r, fill=255)
    return mask


def flame(draw: ImageDraw.ImageDraw, cx: float, base_y: float, h: float, colour) -> None:
    """A teardrop: wide at the base, drawn to a point at the top."""
    w = h * 0.55
    draw.polygon(
        [
            (cx, base_y - h),
            (cx + w / 2, base_y - h * 0.45),
            (cx + w / 2, base_y - h * 0.12),
            (cx, base_y),
            (cx - w / 2, base_y - h * 0.12),
            (cx - w / 2, base_y - h * 0.45),
        ],
        fill=colour,
    )


def lamp(draw: ImageDraw.ImageDraw, cx: float, cy: float, w: float, colour, wick: bool = True) -> None:
    """An oil lamp seen side on: a shallow bowl, a lip, and a wick at its left."""
    bowl_h = w * 0.30
    draw.pieslice([cx - w / 2, cy - bowl_h, cx + w / 2, cy + bowl_h], 0, 180, fill=colour)
    draw.rectangle([cx - w / 2, cy - bowl_h * 0.1, cx + w / 2, cy], fill=colour)
    draw.line([(cx - w / 2 - w * 0.08, cy), (cx - w / 2 + w * 0.10, cy)], fill=colour, width=int(w * 0.1))
    if wick:
        draw.line([(cx - w * 0.34, cy), (cx - w * 0.30, cy - w * 0.16)], fill=colour, width=int(w * 0.06))


def placeholder(size: int = 1024) -> Image.Image:
    """Plain on purpose: a paper square with one lamp, no lighting tricks."""
    img = canvas(size, PAPER)
    d = ImageDraw.Draw(img)
    lamp(d, size * 0.50, size * 0.63, size * 0.52, ACCENT)
    flame(d, size * 0.50 - size * 0.16, size * 0.47, size * 0.17, WARM)
    img.putalpha(rounded_mask(size))
    return img


def draft_a(size: int = 512) -> Image.Image:
    """Lamp, plain: accent ink on paper, the mark alone."""
    img = canvas(size, PAPER)
    d = ImageDraw.Draw(img)
    lamp(d, size * 0.5, size * 0.66, size * 0.58, INK)
    flame(d, size * 0.5 - size * 0.18, size * 0.50, size * 0.19, WARM)
    img.putalpha(rounded_mask(size))
    return img


def draft_b(size: int = 512) -> Image.Image:
    """Lamp at night: a warm pool of light, the page edge lit, everything else dark."""
    img = canvas(size, NIGHT)
    glow = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    cx, cy = size * 0.52, size * 0.44
    for i, (rad, alpha) in enumerate([(0.46, 60), (0.34, 80), (0.22, 110)]):
        r = size * rad
        gd.ellipse([cx - r, cy - r, cx + r, cy + r], fill=(224, 166, 75, alpha))
    glow = glow.filter(ImageFilter.GaussianBlur(size * 0.06))
    img.alpha_composite(glow)
    d = ImageDraw.Draw(img)
    # the page, a lit wedge under the lamp
    d.polygon(
        [(size * 0.10, size * 0.94), (size * 0.90, size * 0.94), (size * 0.74, size * 0.72), (size * 0.26, size * 0.72)],
        fill=(238, 232, 218, 235),
    )
    lamp(d, size * 0.52, size * 0.68, size * 0.34, (250, 245, 232, 255))
    flame(d, size * 0.42, size * 0.60, size * 0.16, WARM)
    img.putalpha(rounded_mask(size))
    return img


def draft_c(size: int = 512) -> Image.Image:
    """Page and wick: the flame is the page's own fold, reading and light as one shape."""
    img = canvas(size, PAPER)
    d = ImageDraw.Draw(img)
    # two pages, slightly apart, the right one lifting into a flame
    d.polygon([(size * 0.16, size * 0.80), (size * 0.48, size * 0.72), (size * 0.48, size * 0.30), (size * 0.16, size * 0.38)], fill=ACCENT)
    d.polygon([(size * 0.52, size * 0.72), (size * 0.84, size * 0.80), (size * 0.84, size * 0.38), (size * 0.52, size * 0.30)], fill=(60, 92, 176, 255))
    flame(d, size * 0.50, size * 0.36, size * 0.26, WARM)
    img.putalpha(rounded_mask(size))
    return img


def main() -> None:
    out = ROOT / "packaging" / "logo"
    out.mkdir(parents=True, exist_ok=True)
    placeholder().save(out / "lamplit-placeholder.png")
    print(f"  {out / 'lamplit-placeholder.png'}")
    for name, fn in [("a", draft_a), ("b", draft_b), ("c", draft_c)]:
        path = Path(f"/tmp/lamplit-draft-{name}.png")
        fn().save(path)
        print(f"  {path}")


if __name__ == "__main__":
    main()
