#!/usr/bin/env python3
"""Draws the disk image's background: community artwork (background-art.jpg) behind the two icons, a rounded arrow in
the brand colours (pink fill, mint halo, like the logo glyph on the app icon) level with them, and the artist's credit.

    python3 packaging/macos/render-background.py        (needs Pillow; the credit uses a macOS font)

Writes background.png (640 x 400) and background@2x.png (1280 x 800) next to this file; packaging/macos.sh joins them
into one Retina TIFF with `tiffutil`. Commit the PNGs together with this script.

The artwork is by Luan Nguyen (artstation.com/artwork/oONQmB), widened on the left and right to fit the window;
background-art.jpg is that copy at 2560 px wide. Finder draws the icon names dark and cannot be told otherwise, so each name sits on a small frosted light pill. The colours of the arrow are the tokens of brand/tokens/colors.json.
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageFont

PINK = (0xAB, 0x4A, 0x81)
MINT = (0xC0, 0xDE, 0xDC)
VEIL = (250, 246, 249, 214)   # the white over the blurred art in each pill
LABEL_Y = 282                 # the centre line of the icon names (Finder puts them under the icons)
LABEL_HALF_W, LABEL_HALF_H = 60, 12
CREDIT = "Artwork by Luan Nguyen"
CREDIT_COLOUR = (232, 222, 240)
W, H = 640, 400                # points; the Finder window is 640 x 400 with the title bar inside it
ICON_X = (170, 470)            # the centres of the two icons (dmg-settings.py)
ICON_Y = 200
SS = 4                         # supersampling for the arrow's smooth edges
ART_FOCUS_Y = 0.5              # which part of the picture shows when it is taller than the window
FONTS = ["/System/Library/Fonts/HelveticaNeue.ttc", "/System/Library/Fonts/Helvetica.ttc", "DejaVuSans.ttf"]


def art(scale: int) -> Image.Image:
    """The artwork, covering the window."""
    w, h = W * scale, H * scale
    img = Image.open(Path(__file__).with_name("background-art.jpg")).convert("RGB")
    r = max(w / img.width, h / img.height)
    img = img.resize((round(img.width * r), round(img.height * r)), Image.LANCZOS)
    x0, y0 = (img.width - w) // 2, round((img.height - h) * ART_FOCUS_Y)
    return ImageEnhance.Brightness(img.crop((x0, y0, x0 + w, y0 + h))).enhance(0.92).convert("RGBA")


def label_pills(scale: int, base: Image.Image) -> Image.Image:
    """A small frosted pill behind each icon name: Finder draws the names dark, so they need a light patch, and the rest of
    the artwork stays clear."""
    out = Image.new("RGBA", base.size, (0, 0, 0, 0))
    mask = Image.new("L", base.size, 0)
    d = ImageDraw.Draw(mask)
    for cx in ICON_X:
        d.rounded_rectangle([(cx - LABEL_HALF_W) * scale, (LABEL_Y - LABEL_HALF_H) * scale,
                             (cx + LABEL_HALF_W) * scale, (LABEL_Y + LABEL_HALF_H) * scale], radius=LABEL_HALF_H * scale, fill=255)
    frosted = base.filter(ImageFilter.GaussianBlur(4 * scale))
    frosted.alpha_composite(Image.new("RGBA", base.size, VEIL))
    out.paste(frosted, (0, 0), mask)
    return out


def credit(scale: int) -> Image.Image:
    out = Image.new("RGBA", (W * scale, H * scale), (0, 0, 0, 0))
    font = None
    for path in FONTS:
        try:
            font = ImageFont.truetype(path, 10 * scale)
            break
        except OSError:
            continue
    font = font or ImageFont.load_default()
    d = ImageDraw.Draw(out)
    box = d.textbbox((0, 0), CREDIT, font=font)
    x, y = (W * scale - (box[2] - box[0])) // 2, 346 * scale
    d.text((x + scale, y + scale), CREDIT, font=font, fill=(0, 0, 0, 150))   # a soft shadow keeps it legible on the art
    d.text((x, y), CREDIT, font=font, fill=CREDIT_COLOUR + (255,))
    return out


def arrow_points(s: float):
    """A block arrow pointing right, centred between the icons and level with them, about 100 points long."""
    cx, cy = (ICON_X[0] + ICON_X[1]) / 2 * s, ICON_Y * s
    half_len, shaft, head_h, head_len = 46 * s, 10 * s, 27 * s, 32 * s
    x0, x1 = cx - half_len, cx + half_len
    xh = x1 - head_len
    return [
        (x0, cy - shaft), (xh, cy - shaft), (xh, cy - head_h), (x1, cy),
        (xh, cy + head_h), (xh, cy + shaft), (x0, cy + shaft),
    ]


def rounded(draw: ImageDraw.ImageDraw, pts, colour, grow: float) -> None:
    """Fills the shape and widens it by `grow` with round corners (a stroke with round joints plus a disc per corner)."""
    draw.polygon(pts, fill=colour)
    draw.line(pts + [pts[0], pts[1]], fill=colour, width=round(2 * grow), joint="curve")
    for x, y in pts:
        draw.ellipse([x - grow, y - grow, x + grow, y + grow], fill=colour)


def arrow(scale: int) -> Image.Image:
    s = scale * SS
    body_round, halo = 5 * s, 5 * s
    pts = arrow_points(s)
    big = Image.new("RGBA", (W * s, H * s), (0, 0, 0, 0))

    shadow = Image.new("RGBA", big.size, (0, 0, 0, 0))   # a soft shadow under the arrow
    rounded(ImageDraw.Draw(shadow), [(x, y + 5 * s) for x, y in pts], (0x20, 0x10, 0x30, 110), body_round + halo)
    big.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(7 * s)))

    d = ImageDraw.Draw(big)
    rounded(d, pts, MINT + (255,), body_round + halo)    # the mint halo
    rounded(d, pts, PINK + (255,), body_round)           # the pink body
    return big.resize((W * scale, H * scale), Image.LANCZOS)


def main() -> None:
    here = Path(__file__).resolve().parent
    for scale, name in ((1, "background.png"), (2, "background@2x.png")):
        img = art(scale)
        for layer in (label_pills(scale, img), arrow(scale), credit(scale)):
            img.alpha_composite(layer)
        img.convert("RGB").save(here / name, dpi=(72 * scale, 72 * scale))
        print("wrote", here / name)


if __name__ == "__main__":
    main()
