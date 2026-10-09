"""Generate the NSIS installer images (header 150x57, sidebar 164x314) as 24-bit BMPs.

Black and orange theme (matches app/src/app.css). The icon is drawn by hand from
app/app-icon.svg's geometry, because Pillow cannot read SVG.

Usage: python make-images.py     (requires Pillow: pip install Pillow)
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

OUT = Path(__file__).resolve().parent
ORANGE = (255, 163, 26)
BLACK = (0, 0, 0)
MUTED = (160, 160, 160)
SS = 4  # supersampling factor for smooth edges


def font(size, bold=True):
    names = ["segoeuib.ttf", "arialbd.ttf"] if bold else ["segoeui.ttf", "arial.ttf"]
    for name in names:
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def line(d, pts, width, fill):
    """Polyline with round caps and joins."""
    d.line(pts, fill=fill, width=width, joint="curve")
    r = width / 2
    for x, y in (pts[0], pts[-1]):
        d.ellipse([x - r, y - r, x + r, y + r], fill=fill)


def icon(size):
    """The ODM app icon at size x size (RGB, on black)."""
    s = size * SS
    img = Image.new("RGB", (s, s), BLACK)
    d = ImageDraw.Draw(img)
    k = s / 1024
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=220 * k, fill=ORANGE)
    w = round(84 * k)
    line(d, [(512 * k, 232 * k), (512 * k, 620 * k)], w, BLACK)
    line(d, [(344 * k, 468 * k), (512 * k, 636 * k), (680 * k, 468 * k)], w, BLACK)
    line(d, [(288 * k, 772 * k), (736 * k, 772 * k)], w, BLACK)
    return img.resize((size, size), Image.LANCZOS)


def header():
    img = Image.new("RGB", (150, 57), BLACK)
    img.paste(icon(36), (108, 10))
    d = ImageDraw.Draw(img)
    d.text((10, 14), "ODM", font=font(22), fill=ORANGE)
    d.rectangle([0, 55, 149, 56], fill=ORANGE)
    return img


def sidebar():
    img = Image.new("RGB", (164, 314), BLACK)
    img.paste(icon(96), (34, 48))
    d = ImageDraw.Draw(img)
    for text, y, f, fill in [
        ("ODM", 168, font(34), ORANGE),
        ("Open Downloader", 214, font(13, False), (255, 255, 255)),
        ("Manager", 232, font(13, False), (255, 255, 255)),
    ]:
        w = d.textlength(text, font=f)
        d.text(((164 - w) / 2, y), text, font=f, fill=fill)
    d.rectangle([0, 0, 3, 313], fill=ORANGE)
    return img


if __name__ == "__main__":
    header().save(OUT / "header.bmp", format="BMP")
    sidebar().save(OUT / "sidebar.bmp", format="BMP")
    print("wrote", OUT / "header.bmp", OUT / "sidebar.bmp")
