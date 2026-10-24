#!/usr/bin/env python3
"""Regenerates the pictures of the `stone` example skin. Original art, made from arithmetic (no third-party images).
Run:  python3 make-art.py   (writes frame.png, stone.png, gem.png next to this file)"""
import os, struct, zlib

HERE = os.path.dirname(os.path.abspath(__file__))


def png(path, w, h, px):
    """px(x, y) -> (r, g, b, a); writes an 8-bit RGBA PNG."""
    raw = b"".join(b"\x00" + b"".join(bytes(px(x, y)) for x in range(w)) for y in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(os.path.join(HERE, path), "wb") as f:
        f.write(data)


def noise(x, y, seed):
    """Small deterministic hash noise in 0..255."""
    n = (x * 374761393 + y * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return (n ^ (n >> 16)) & 0xFF


def stone(x, y):
    # tiles seamlessly: noise indexes wrap at 32
    v = 38 + noise(x % 32, y % 32, 1) // 12 + noise(x // 2 % 16, y // 2 % 16, 2) // 10
    return (v, v + 2, v + 6, 255)


def frame(x, y):
    n, e = 24, 8  # 24 px square, 8 px border
    d = min(x, y, n - 1 - x, n - 1 - y)
    if d >= e:
        return (30, 27, 24, 255)  # the dark parchment the text sits on
    lit = (x + y) < n  # light from the top left
    base = 96 if lit else 58
    ring = 10 if d in (0, e - 1) else 0  # an engraved line on the outer and inner edge
    v = base + noise(x, y, 3) // 16 - ring * (1 if d == e - 1 else -1)
    return (v, v + 1, v + 5, 255)


def gem(x, y):
    c, r = 7.5, 7.0
    dist = abs(x - c) + abs(y - c)
    if dist > r:
        return (0, 0, 0, 0)
    shade = 150 + int(80 * (1 - dist / r)) - (30 if x > c else 0)
    return (shade // 3, shade, shade // 2 + 40, 255 if dist < r - 0.5 else 160)


png("stone.png", 32, 32, stone)
png("frame.png", 24, 24, frame)
png("gem.png", 16, 16, gem)
