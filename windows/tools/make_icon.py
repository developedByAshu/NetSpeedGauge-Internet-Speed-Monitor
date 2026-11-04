"""Draws NetSpeedGauge's app icon and saves it to windows/assets/netspeedgauge.ico and shared/assets/netspeedgauge.png.

Run with any Python that has Pillow: python windows/tools/make_icon.py
"""
from pathlib import Path

from PIL import Image, ImageDraw

MASTER_SIZE = 1024
ICON_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
ICON_FOLDER = Path(__file__).resolve().parent.parent / "assets"
PICTURE_FOLDER = Path(__file__).resolve().parents[2] / "shared" / "assets"
TOP_COLOR = (64, 142, 255)
BOTTOM_COLOR = (20, 74, 200)
UP_ARROW_COLOR = (255, 255, 255, 255)
DOWN_ARROW_COLOR = (176, 216, 255, 255)
CORNER_RADIUS = 0.225
# Size of the arrows relative to the icon: small sizes need thicker, larger shapes to stay readable.
SHAFT_WIDTH = 0.135
HEAD_WIDTH = 0.34
HEAD_HEIGHT = 0.2
TOP_EDGE = 0.17
BOTTOM_EDGE = 0.83
UP_ARROW_CENTER = 0.32
DOWN_ARROW_CENTER = 0.68


def gradient(size):
    """Returns a square fading from the top colour to the bottom colour."""
    image = Image.new("RGB", (1, size))
    for row in range(size):
        amount = row / (size - 1)
        image.putpixel((0, row), tuple(round(a + (b - a) * amount) for a, b in zip(TOP_COLOR, BOTTOM_COLOR)))
    return image.resize((size, size))


def arrow_points(center, is_up, size):
    """Returns the outline of one arrow."""
    half_shaft = SHAFT_WIDTH / 2
    half_head = HEAD_WIDTH / 2
    if is_up:
        points = [
            (center, TOP_EDGE),
            (center + half_head, TOP_EDGE + HEAD_HEIGHT),
            (center + half_shaft, TOP_EDGE + HEAD_HEIGHT),
            (center + half_shaft, BOTTOM_EDGE),
            (center - half_shaft, BOTTOM_EDGE),
            (center - half_shaft, TOP_EDGE + HEAD_HEIGHT),
            (center - half_head, TOP_EDGE + HEAD_HEIGHT),
        ]
    else:
        points = [
            (center, BOTTOM_EDGE),
            (center + half_head, BOTTOM_EDGE - HEAD_HEIGHT),
            (center + half_shaft, BOTTOM_EDGE - HEAD_HEIGHT),
            (center + half_shaft, TOP_EDGE),
            (center - half_shaft, TOP_EDGE),
            (center - half_shaft, BOTTOM_EDGE - HEAD_HEIGHT),
            (center - half_head, BOTTOM_EDGE - HEAD_HEIGHT),
        ]
    return [(x * size, y * size) for x, y in points]


def draw_master():
    """Draws the icon at a large size, to be shrunk smoothly to every size Windows asks for."""
    size = MASTER_SIZE
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1), radius=size * CORNER_RADIUS, fill=255)
    icon = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    icon.paste(gradient(size), (0, 0), mask)
    draw = ImageDraw.Draw(icon)
    draw.polygon(arrow_points(UP_ARROW_CENTER, True, size), fill=UP_ARROW_COLOR)
    draw.polygon(arrow_points(DOWN_ARROW_CENTER, False, size), fill=DOWN_ARROW_COLOR)
    return icon


def main():
    ICON_FOLDER.mkdir(exist_ok=True)
    PICTURE_FOLDER.mkdir(exist_ok=True)
    master = draw_master()
    master.resize((256, 256), Image.LANCZOS).save(PICTURE_FOLDER / "netspeedgauge.png")
    master.save(ICON_FOLDER / "netspeedgauge.ico", sizes=[(size, size) for size in ICON_SIZES])
    print((ICON_FOLDER / "netspeedgauge.ico").stat().st_size, "bytes")


main()
