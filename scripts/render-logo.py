"""Build the PNG and Windows ICO from assets/icon/sleepchord.svg with Pillow."""

from io import BytesIO
from pathlib import Path
import struct
import xml.etree.ElementTree as ET

from PIL import Image, ImageDraw


ASSETS = Path(__file__).resolve().parents[1] / "assets" / "icon"
SIZES = (16, 24, 32, 48, 64, 128, 256)
SVG = "{http://www.w3.org/2000/svg}"


def render(master, size):
    # Rasterize independently at every output size with supersampling.
    _, _, view_width, _ = map(float, master.attrib["viewBox"].split())
    scale = size * 8 / view_width
    image = Image.new("RGBA", (size * 8, size * 8))
    draw = ImageDraw.Draw(image)
    rect = master.find(SVG + "rect").attrib
    x, y, width, height, radius = (
        float(rect[key]) * scale for key in ("x", "y", "width", "height", "rx")
    )
    draw.rounded_rectangle((x, y, x + width, y + height), radius, fill=rect["fill"])

    wave = master.find(SVG + "g")
    radius = float(wave.attrib["stroke-width"]) * scale / 2
    for line in wave.findall(SVG + "line"):
        x, top, bottom = (float(line.attrib[key]) * scale for key in ("x1", "y1", "y2"))
        draw.rounded_rectangle(
            (x - radius, top - radius, x + radius, bottom + radius),
            radius,
            fill=wave.attrib["stroke"],
        )
    return image.resize((size, size), Image.Resampling.BOX)


def main():
    master = ET.parse(ASSETS / "sleepchord.svg").getroot()
    render(master, 512).save(ASSETS / "icon.png")

    entries, images = [], []
    offset = 6 + 16 * len(SIZES)
    for size in SIZES:
        buffer = BytesIO()
        render(master, size).save(buffer, format="PNG")
        png = buffer.getvalue()
        dimensions = size % 256  # ICO represents the 256px dimension with a zero byte.
        entries.append(struct.pack("<BBBBHHII", dimensions, dimensions, 0, 0, 1, 32, len(png), offset))
        images.append(png)
        offset += len(png)
    ico = struct.pack("<HHH", 0, 1, len(SIZES)) + b"".join(entries + images)
    (ASSETS / "icon.ico").write_bytes(ico)


if __name__ == "__main__":
    main()
