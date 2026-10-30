#!/usr/bin/env python3
"""Write the pictures the store's tests measure.

Everything here is stdlib: a PNG is four chunks and a zlib stream, a BMP is a
header and three bytes a pixel, and a GIF is old enough to be written by hand.
The fixtures are committed, so a test never builds one at run time; this script
is how they were made and how they are made different (image 57).
"""

import struct
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent


def chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def ihdr(width: int, height: int) -> bytes:
    # 8-bit truecolour, no interlace: three bytes a pixel, no palette to carry.
    return chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))


def rows(width: int, height: int) -> bytes:
    """A picture and not a rectangle of one colour: a gradient left to right,
    so a decoder that drops the pixels cannot accidentally look right."""
    out = bytearray()
    for _ in range(height):
        out.append(0)  # filter: none
        for x in range(width):
            out += bytes(((x * 255) // max(width - 1, 1), 96, 160))
    return bytes(out)


def png(path: Path, width: int, height: int) -> None:
    idat = zlib.compress(rows(width, height), 9)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + ihdr(width, height)
        + chunk(b"IDAT", idat)
        + chunk(b"IEND", b"")
    )


def bmp(path: Path, width: int, height: int) -> None:
    """An image the decoders may read and the store refuses by name."""
    stride = (width * 3 + 3) & ~3
    pixels = b"".join(b"\x20\x40\x80" * width + b"\x00" * (stride - width * 3) for _ in range(height))
    header = struct.pack("<2sIHHI", b"BM", 14 + 40 + len(pixels), 0, 0, 14 + 40)
    info = struct.pack("<IiiHHIIiiII", 40, width, height, 1, 24, 0, len(pixels), 2835, 2835, 0, 0)
    path.write_bytes(header + info + pixels)


def main() -> None:
    # A wide picture: three columns of picture to one of height.
    png(HERE / "wide.png", 600, 200)
    # A square, so a cell twice as tall as it is wide can be shown to keep it.
    png(HERE / "square.png", 200, 200)
    # Narrower than any block of columns it could be asked for.
    png(HERE / "narrow.png", 100, 40)
    # Thousands of rows tall: the cap is what stops this one, not its aspect.
    png(HERE / "tall.png", 10, 5000)

    # A header a reader can measure, and a body no decoder could: the IDAT is
    # noise, so measuring this file proves the pixels were never read.
    (HERE / "corrupt.png").write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + ihdr(100, 50)
        + chunk(b"IDAT", b"not a zlib stream, and proud of it" * 4)
        + chunk(b"IEND", b"")
    )
    # The same, cut off inside the header: not even a size to be had.
    whole = (HERE / "square.png").read_bytes()
    (HERE / "truncated.png").write_bytes(whole[:20])

    # One small file that claims a hundred billion pixels: refused before any
    # decoder allocates for it. The body is a stream of a thousand zeros, so the
    # header is one a reader believes and the pixels are ones no decoder could
    # ever produce.
    (HERE / "bomb.png").write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + ihdr(100_000, 100_000)
        + chunk(b"IDAT", zlib.compress(b"\x00" * 1000))
        + chunk(b"IEND", b"")
    )

    # A format the decoders link and the store does not draw.
    bmp(HERE / "map.bmp", 2, 2)
    # Bytes that are not an image at all, and say so.
    (HERE / "notes.txt").write_text("These are words, not pixels.\n")


if __name__ == "__main__":
    main()
