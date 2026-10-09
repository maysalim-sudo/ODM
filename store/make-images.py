"""Generate store/images/promo-440x280.png and icon-128.png. Needs Pillow."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "store" / "images"
ICON = ROOT / "app" / "src-tauri" / "icons" / "128x128.png"
FONTS = Path("C:/Windows/Fonts")


def font(size):
    for name in ("segoeuib.ttf", "arialbd.ttf"):
        try:
            return ImageFont.truetype(str(FONTS / name), size)
        except OSError:
            pass
    return ImageFont.load_default()


def fit(text, size, maxw, d):
    while size > 8 and d.textlength(text, font=font(size)) > maxw:
        size -= 1
    return font(size)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    icon = Image.open(ICON).convert("RGBA")
    icon.save(OUT / "icon-128.png")

    w, h = 440, 280
    img = Image.new("RGB", (w, h), "#000000")
    d = ImageDraw.Draw(img)
    big = icon.resize((96, 96), Image.LANCZOS)
    img.paste(big, (40, 52), big)
    d.text((156, 50), "ODM", font=font(72), fill="#ffffff")
    d.text((40, 176), "Downloads that split,", font=fit("Downloads that split,", 26, 360, d), fill="#a0a0a0")
    d.text((40, 210), "resume and finish.", font=fit("resume and finish.", 26, 360, d), fill="#a0a0a0")
    img.save(OUT / "promo-440x280.png")


if __name__ == "__main__":
    main()
