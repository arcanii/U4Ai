"""Writes assets/icon/u4ai.ico from the PNGs beside it.

    python3 -I assets/icon/make-ico.py

An ICO may hold a PNG for each of its images (Windows Vista and later read
them), so the 16, 32, 64, 128 and 256 pixel PNGs go in as they are, the
smallest first, and the file comes out the same each time. The installer's
icon (windows/packaging/installer.iss) and the Start menu's are this file.
Run it again whenever the PNGs change.
"""

import pathlib
import struct
import sys

SIZES = (16, 32, 64, 128, 256)
HERE = pathlib.Path(__file__).resolve().parent
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def read_png(size):
    """The PNG's bytes, checked to be a square of `size` pixels in RGBA."""
    path = HERE / f"{size}.png"
    data = path.read_bytes()
    if data[:8] != PNG_SIGNATURE or data[12:16] != b"IHDR":
        sys.exit(f"{path} isn't a PNG")
    width, height, depth, color = struct.unpack(">IIBB", data[16:26])
    if (width, height) != (size, size):
        sys.exit(f"{path} is {width} x {height}, not {size} x {size}")
    if (depth, color) != (8, 6):
        sys.exit(f"{path} isn't 8-bit RGBA (bit depth {depth}, color type {color})")
    return data


def main():
    images = [(size, read_png(size)) for size in SIZES]
    # ICONDIR, then an ICONDIRENTRY of 16 bytes each, then the images.
    out = bytearray(struct.pack("<HHH", 0, 1, len(images)))
    offset = 6 + 16 * len(images)
    for size, data in images:
        side = 0 if size == 256 else size  # a byte; 0 says 256
        out += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    for _, data in images:
        out += data
    target = HERE / "u4ai.ico"
    target.write_bytes(bytes(out))
    print(f"wrote {target} ({len(out)} bytes, {', '.join(str(s) for s in SIZES)} px)")


if __name__ == "__main__":
    main()
