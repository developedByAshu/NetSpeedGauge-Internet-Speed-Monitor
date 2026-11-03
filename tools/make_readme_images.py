"""Tidies the window screenshots in docs/images and draws the banner, which is also GitHub's social preview.

Run with any Python that has Pillow: python tools/make_readme_images.py
"""
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
IMAGES = ROOT / "docs" / "images"
WINDOW_SHOTS = ["usage.png", "settings-appearance.png", "settings-details.png", "hover-card.png"]
FONT_REGULAR = "C:/Windows/Fonts/segoeui.ttf"
FONT_STRONG = "C:/Windows/Fonts/seguisb.ttf"
BANNER_SIZE = (1280, 640)
BACKGROUND_TOP = (13, 19, 33)
BACKGROUND_BOTTOM = (22, 38, 70)
CARD_COLOR = (32, 45, 74)
TEXT_COLOR = (245, 247, 250)
MUTED_COLOR = (170, 184, 208)
ACCENT_COLOR = (96, 165, 250)
SCALE = 4
LEFT_MARGIN = 64
STATS = [("Under 4 MB", "RAM"), ("0.013%", "CPU"), ("0", "connections")]


def rounded(image, radius):
    """Returns the image with smooth rounded corners."""
    mask = Image.new("L", (image.width * SCALE, image.height * SCALE), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, mask.width - 1, mask.height - 1), radius * SCALE, fill=255)
    result = image.convert("RGBA")
    result.putalpha(mask.resize(image.size, Image.LANCZOS))
    return result


def tidy_window_shot(path):
    """Cuts away the black margin a window capture leaves around the window, then rounds the corners."""
    image = Image.open(path).convert("RGB")
    box = ImageChops.difference(image, Image.new("RGB", image.size, (0, 0, 0))).getbbox()
    rounded(image.crop(box), 10).save(path)


def draw_banner():
    """Draws the banner from the tidied screenshots, with the measured numbers beside the usage window."""
    banner = Image.new("RGB", BANNER_SIZE)
    draw = ImageDraw.Draw(banner)
    for row in range(BANNER_SIZE[1]):
        amount = row / BANNER_SIZE[1]
        color = tuple(round(a + (b - a) * amount) for a, b in zip(BACKGROUND_TOP, BACKGROUND_BOTTOM))
        draw.line((0, row, BANNER_SIZE[0], row), fill=color)

    icon = Image.open(ROOT / "assets" / "netspeedgauge.png").convert("RGBA").resize((72, 72), Image.LANCZOS)
    banner.paste(icon, (LEFT_MARGIN, 72), icon)
    draw.text((LEFT_MARGIN + 88, 70), "NetSpeedGauge", font=ImageFont.truetype(FONT_STRONG, 56), fill=TEXT_COLOR)
    tagline = ImageFont.truetype(FONT_REGULAR, 28)
    draw.text((LEFT_MARGIN, 172), "Internet speed monitor for the", font=tagline, fill=TEXT_COLOR)
    draw.text((LEFT_MARGIN, 210), "Windows taskbar", font=tagline, fill=TEXT_COLOR)

    value_font = ImageFont.truetype(FONT_STRONG, 30)
    label_font = ImageFont.truetype(FONT_REGULAR, 19)
    card_width, card_height, gap = 196, 100, 14
    for position, (value, label) in enumerate(STATS):
        x = LEFT_MARGIN + position * (card_width + gap)
        draw.rounded_rectangle((x, 280, x + card_width, 280 + card_height), 14, fill=CARD_COLOR)
        draw.text((x + 18, 296), value, font=value_font, fill=ACCENT_COLOR)
        draw.text((x + 18, 342), label, font=label_font, fill=MUTED_COLOR)

    strip = Image.open(IMAGES / "taskbar.png").convert("RGB")
    strip = strip.crop((60, 0, strip.width, strip.height)).resize(((strip.width - 60) * 2, strip.height * 2), Image.LANCZOS)
    banner.paste(rounded(strip, 12), (LEFT_MARGIN, 410), rounded(strip, 12))

    draw.text((LEFT_MARGIN, 530), "Free and open source. Never connects to the internet.", font=label_font, fill=MUTED_COLOR)
    draw.text((LEFT_MARGIN, 562), "Measured on Windows 11. Usage window shows sample data.", font=ImageFont.truetype(FONT_REGULAR, 15), fill=MUTED_COLOR)

    usage = Image.open(IMAGES / "usage.png").convert("RGBA")
    height = 560
    usage = usage.resize((round(usage.width * height / usage.height), height), Image.LANCZOS)
    banner.paste(usage, (BANNER_SIZE[0] - usage.width - 56, 40), usage)
    banner.save(IMAGES / "banner.png", optimize=True)


if __name__ == "__main__":
    for name in WINDOW_SHOTS:
        tidy_window_shot(IMAGES / name)
    draw_banner()
