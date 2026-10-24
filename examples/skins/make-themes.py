#!/usr/bin/env python3
"""Generates the pictures of the `parchment`, `midnight` and `lavender` example skins. Original art made from arithmetic.
Run:  python3 make-themes.py   (writes PNGs into the three folders beside this file)"""
import math, os, struct, zlib

HERE = os.path.dirname(os.path.abspath(__file__))


def png(folder, name, w, h, px):
    os.makedirs(os.path.join(HERE, folder), exist_ok=True)
    raw = b"".join(b"\x00" + b"".join(bytes(px(x, y)) for x in range(w)) for y in range(h))
    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(os.path.join(HERE, folder, name), "wb") as f:
        f.write(data)


def noise(x, y, seed):
    n = (x * 374761393 + y * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    n = ((n ^ (n >> 13)) * 1274126177) & 0xFFFFFFFF
    return (n ^ (n >> 16)) & 0xFF


def clamp(v):
    return max(0, min(255, int(v)))


def mix(a, b, t):
    return tuple(clamp(a[i] + (b[i] - a[i]) * t) for i in range(3))


# ---------------------------------------------------------------- parchment (light, warm, wood and gold)
def parchment_page(x, y):
    n, e = 32, 10
    d = min(x, y, n - 1 - x, n - 1 - y)
    if d >= e:  # parchment, with fibres and a vignette toward the frame
        v = noise(x, y, 5) // 24 + noise(x // 2, y // 3, 6) // 20
        k = min(1.0, (d - e) / 6.0)
        r, g, b = mix((226, 206, 160), (246, 236, 210), k)
        return (clamp(r - v), clamp(g - v), clamp(b - v * 1.4), 255)
    grain = (math.sin(y * 1.7 + noise(x // 4, 0, 7) / 40.0) + 1) * 7  # horizontal grain on the top, same on sides by symmetry
    lit = (x + y) < n
    base = (118, 78, 44) if lit else (84, 52, 28)
    if d == 0:
        base = (60, 36, 18)
    if d == e - 1:  # gold inlay line
        base = (201, 160, 72)
    v = grain + noise(x, y, 8) // 20
    return (clamp(base[0] + v), clamp(base[1] + v * 0.8), clamp(base[2] + v * 0.5), 255)


def parchment_side(x, y):  # leather
    v = noise(x % 32, y % 32, 9) // 9 + noise(x // 3 % 11, y // 3 % 11, 10) // 6
    return (clamp(204 - v), clamp(176 - v), clamp(136 - v), 255)


def corner_flourish(x, y):  # an L bracket with a curl and a bead, gold on transparent; top-left corner
    gold = (214, 170, 70)
    shade = (150, 110, 40)
    on = x < 3 and y < 26 or y < 3 and x < 26
    on = on or (x - 7) ** 2 + (y - 7) ** 2 <= 5  # bead
    cx, cy = 14.0, 14.0
    ring = abs(math.hypot(x - cx, y - cy) - 5.0) < 1.0 and x > 9 and y > 9 and (x < 19 or y < 19)
    if on or ring:
        return (*(gold if (x + y) % 5 else shade), 255)
    return (0, 0, 0, 0)


def flipped(f, w, h, fx, fy):
    return lambda x, y: f(w - 1 - x if fx else x, h - 1 - y if fy else y)


# ---------------------------------------------------------------- midnight (dark, navy, cyan glow)
def midnight_page(x, y):
    n, e = 16, 5
    d = min(x, y, n - 1 - x, n - 1 - y)
    if d >= e:
        t = 0.0
        return (12, 16, 30, 255)
    glow = [255, 190, 110, 70, 38][d]  # alpha falls off inward
    if d == 0:
        return (80, 220, 255, 255)
    return (40, 150, 220, glow)


def midnight_side(x, y):  # hex-ish circuit grid
    on = x % 16 == 0 or y % 16 == 0
    node = x % 16 in (0, 1) and y % 16 in (0, 1)
    base = 22 + noise(x, y, 11) // 40
    if node:
        return (60, 170, 220, 255)
    if on:
        return (28, 48, 78, 255)
    return (base, base + 4, base + 18, 255)


def bracket(x, y):  # cyan corner bracket, top-left
    if (x < 2 and y < 18) or (y < 2 and x < 18) or (x < 5 and y < 5 and x + y < 7):
        return (90, 230, 255, 255)
    return (0, 0, 0, 0)


# ---------------------------------------------------------------- lavender (light, purple)
def lavender_page(x, y):
    n, e = 16, 6
    d = min(x, y, n - 1 - x, n - 1 - y)
    if d >= e:
        return (255, 255, 255, 255)
    shadow = [235, 224, 214, 208, 206, 214][d] if (x > 8 or y > 8) else [200, 214, 228, 238, 246, 252][d]
    return (shadow, shadow, min(255, shadow + 4), 255)


def lavender_side(x, y):  # soft lavender paper with a fine weave
    v = 244 - ((x + y) % 2) * 3 - noise(x, y, 12) // 64
    return (clamp(v - 6), clamp(v - 8), clamp(v + 2), 255)


png("parchment", "page.png", 32, 32, parchment_page)
png("parchment", "leather.png", 32, 32, parchment_side)
png("parchment", "flourish-tl.png", 28, 28, corner_flourish)
png("parchment", "flourish-br.png", 28, 28, flipped(corner_flourish, 28, 28, True, True))
png("midnight", "page.png", 16, 16, midnight_page)
png("midnight", "circuit.png", 32, 32, midnight_side)
png("midnight", "bracket-tl.png", 20, 20, bracket)
png("midnight", "bracket-br.png", 20, 20, flipped(bracket, 20, 20, True, True))
png("lavender", "page.png", 16, 16, lavender_page)
png("lavender", "lavender.png", 32, 32, lavender_side)
