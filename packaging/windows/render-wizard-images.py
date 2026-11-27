#!/usr/bin/env python3
"""Draws the Windows installer's wizard images from the same community artwork as the macOS disk image.

    python3 packaging/windows/render-wizard-images.py        (needs Pillow; the credit uses a macOS or DejaVu font)

Writes next to this file, as 24-bit BMPs (the format Inno Setup reads):
  wizard-<w>.bmp        the tall picture on the welcome and finish pages, for 100%, 150% and 200% display scaling
                        (164x314, 246x459, 328x604: the sizes the Inno Setup help lists); the artwork and its credit
  wizard-small-<n>.bmp  the square logo at the top right of the other pages (58, 77, 97 and 124 px)
skradle.iss lists them in WizardImageFile and WizardSmallImageFile; Setup picks the one that suits the display.
Commit the BMPs together with this script. The artwork is by Luan Nguyen (artstation.com/artwork/oONQmB), widened to
fit; see packaging/macos/render-background.py.
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageEnhance, ImageFont

HERE = Path(__file__).resolve().parent
ART = HERE.parent / "macos" / "background-art.jpg"
ICON = HERE.parent.parent / "assets" / "brand" / "icon-1024.png"
CREDIT = "Artwork by Luan Nguyen"
CREDIT_COLOUR = (236, 228, 244)
LARGE = [(164, 314), (246, 459), (328, 604)]
SMALL = [58, 77, 97, 124]
FONTS = ["/System/Library/Fonts/HelveticaNeue.ttc", "/System/Library/Fonts/Helvetica.ttc", "DejaVuSans.ttf"]


def font(px: int) -> ImageFont.FreeTypeFont:
    for path in FONTS:
        try:
            return ImageFont.truetype(path, px)
        except OSError:
            continue
    return ImageFont.load_default()


def large(w: int, h: int) -> Image.Image:
    art = Image.open(ART).convert("RGB")
    crop_w = round(art.height * w / h)              # the middle of the picture, as tall as it is
    x0 = (art.width - crop_w) // 2
    img = art.crop((x0, 0, x0 + crop_w, art.height)).resize((w, h), Image.LANCZOS)
    img = ImageEnhance.Brightness(img).enhance(0.95).convert("RGBA")
    # a dark fade at the foot keeps the credit legible on any part of the art
    fade = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(fade)
    top = round(h * 0.86)
    for y in range(top, h):
        d.line([(0, y), (w, y)], fill=(18, 8, 28, round(200 * (y - top) / (h - top))))
    img.alpha_composite(fade)
    f = font(max(9, round(w * 0.062)))
    d = ImageDraw.Draw(img)
    box = d.textbbox((0, 0), CREDIT, font=f)
    d.text(((w - (box[2] - box[0])) // 2, h - round(h * 0.035) - (box[3] - box[1])), CREDIT, font=f, fill=CREDIT_COLOUR + (255,))
    return img.convert("RGB")


def small(n: int) -> Image.Image:
    return Image.open(ICON).convert("RGB").resize((n, n), Image.LANCZOS)


def main() -> None:
    for w, h in LARGE:
        large(w, h).save(HERE / f"wizard-{w}.bmp")
    for n in SMALL:
        small(n).save(HERE / f"wizard-small-{n}.bmp")
    print("wrote", ", ".join(sorted(p.name for p in HERE.glob("wizard-*.bmp"))))


if __name__ == "__main__":
    main()
