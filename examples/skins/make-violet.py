#!/usr/bin/env python3
"""Generates the pictures of the `violet` example skin: widget backgrounds (section tabs, list rows, panes, status
bar, scroll thumb), 16 px icons and 32 px coloured ribbon command icons. Original art, not copied from any product:
every picture is drawn here from arithmetic (rectangles, circles, line segments); no Microsoft art, logos or icons.
Run:  python3 make-violet.py   (writes PNGs into violet/ beside this file; deterministic)"""
import os, struct, zlib

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "violet")

# The colours of the look (also in skin.conf), sampled from a notebook app window.
ACCENT = (0x80, 0x39, 0x7B)  # notebook purple: title bar, selected text
RAIL = (0x5C, 0x28, 0x58)  # a darker purple (the notebook icon's spine)
PASTEL = (0xD6, 0xA6, 0xD3)  # the band under the section tabs, the Add Page bar, the active tab
PANE = (0xF1, 0xF1, 0xF1)  # ribbon, status bar
LINE = (0xD9, 0xD9, 0xD9)  # hairlines
FRAME = (0xC8, 0xC8, 0xC8)  # the frame of the selected page row
TAB = (0xEE, 0xEE, 0xEE)  # an inactive section tab
HOVER = (0xE6, 0xE6, 0xE6)  # list row under the pointer
WHITE = (0xFF, 0xFF, 0xFF)
INK = (0x60, 0x60, 0x60)  # icon strokes
THUMB = (0xC2, 0xC2, 0xC2)
THUMB_HOVER = (0xA0, 0xA0, 0xA0)
CLEAR = None


def png(name, w, h, px):
    """px(x, y) -> (r, g, b, a); writes an 8-bit RGBA PNG."""
    os.makedirs(os.path.dirname(os.path.join(OUT, name)), exist_ok=True)
    raw = b"".join(b"\x00" + b"".join(bytes(px(x, y)) for x in range(w)) for y in range(h))

    def chunk(t, d):
        c = struct.pack(">I", len(d)) + t + d
        return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    with open(os.path.join(OUT, name), "wb") as f:
        f.write(data)


def smooth(shape, n=4):
    """Antialiases `shape(fx, fy) -> (r, g, b) or None` (float coordinates) by n x n supersampling."""

    def px(x, y):
        acc, hit = [0.0, 0.0, 0.0], 0
        for j in range(n):
            for i in range(n):
                c = shape(x + (i + 0.5) / n, y + (j + 0.5) / n)
                if c is not None:
                    hit += 1
                    for k in range(3):
                        acc[k] += c[k]
        if hit == 0:
            return (0, 0, 0, 0)
        return (round(acc[0] / hit), round(acc[1] / hit), round(acc[2] / hit), round(255 * hit / (n * n)))

    return px


def in_round_rect(fx, fy, x0, y0, x1, y1, r, top_only=False):
    """Inside the rectangle x0..x1, y0..y1 with corner radius r (only the top corners when top_only)."""
    if not (x0 <= fx < x1 and y0 <= fy < y1):
        return False
    # the nearest point of the rectangle shrunk by r; inside when within r of it
    cx = min(max(fx, x0 + r), x1 - r)
    cy = max(fy, y0 + r) if top_only else min(max(fy, y0 + r), y1 - r)
    return (fx - cx) ** 2 + (fy - cy) ** 2 <= r * r


def seg_dist(px_, py_, ax, ay, bx, by):
    """Distance from a point to the segment a..b."""
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px_ - ax) * dx + (py_ - ay) * dy) / (dx * dx + dy * dy)))
    return ((px_ - ax - t * dx) ** 2 + (py_ - ay - t * dy) ** 2) ** 0.5


# ---------------------------------------------------------------- section tabs: 16 x 32, flat with rounded tops
# A 2 px transparent gap on the right separates neighbours; the strip shows white above the tabs. Inactive tabs start
# lower than the active one and stop above the 4 px band under the strip; the active tab runs into the band. With
# `ui.sectiontab.fill = 1` only the shape counts: each tab is drawn in its section colour through the transparency.
def tab(fill, top, bottom):
    return smooth(lambda fx, fy: fill if in_round_rect(fx, fy, 0, top, 14, bottom, 4, top_only=True) else CLEAR)


png("tab.png", 16, 32, tab(TAB, 8, 28))
png("tab-active.png", 16, 32, tab(PASTEL, 3, 32))


# ---------------------------------------------------------------- buttons: 16 x 16, slice 5 4 1 4
# The Add Page bar and the section strip's "+" and arrows: a pastel tab with rounded top corners and a clear top edge.
def button(fill):
    return smooth(lambda fx, fy: fill if in_round_rect(fx, fy, 0, 2, 16, 16, 3, top_only=True) else CLEAR)


png("button.png", 16, 16, button(PASTEL))
png("button-hover.png", 16, 16, button((0xCB, 0x94, 0xC7)))
png("button-pressed.png", 16, 16, button((0xBD, 0x80, 0xB8)))


# ---------------------------------------------------------------- list rows: 8 x 8
def row_hover(x, y):  # slice 0 0 0 0: a flat grey row
    return (*HOVER, 255)


def row_selected(x, y):  # slice 1 1 1 1: a white row in a thin grey frame
    return (*(FRAME if x in (0, 7) or y in (0, 7) else WHITE), 255)


png("row-hover.png", 8, 8, row_hover)
png("row-selected.png", 8, 8, row_selected)


# ---------------------------------------------------------------- bars
def strip(x, y):  # 8 x 8, slice 0 0 4 0: the white section strip over a 4 px pastel band
    return (*(PASTEL if y >= 4 else WHITE), 255)


def status(x, y):  # 8 x 8, slice 1 0 0 0: the status bar with a hairline over it
    return (*(LINE if y == 0 else PANE), 255)


png("sections.png", 8, 8, strip)
png("status.png", 8, 8, status)


# ---------------------------------------------------------------- scroll thumb: 10 x 20, slice 5 3 5 3, a pill
def thumb(fill):
    return smooth(lambda fx, fy: fill if in_round_rect(fx, fy, 3, 1, 7, 19, 2) else CLEAR)


png("thumb.png", 10, 20, thumb(THUMB))
png("thumb-hover.png", 10, 20, thumb(THUMB_HOVER))


# ---------------------------------------------------------------- icons: 16 x 16
def notebook_icon(fx, fy):  # a closed purple notebook: dark spine, body, light page edge on the right
    if not in_round_rect(fx, fy, 3, 1, 14, 15, 1.5):
        return None
    if fx < 5.5:
        return RAIL
    if fx > 12.2:
        return (0xE8, 0xD4, 0xE6)
    if 7 <= fx < 11 and 4 <= fy < 6:  # the label on the cover
        return WHITE
    return ACCENT


def section_icon(fx, fy):  # a small tab over a sheet
    if in_round_rect(fx, fy, 2, 3, 8, 6, 1, top_only=True):
        return ACCENT
    if in_round_rect(fx, fy, 2, 5, 14, 14, 1.5):
        return (0xB0, 0x7A, 0xAC) if fy < 6.5 else (0xD9, 0xBF, 0xD6)
    return None


def page_icon(fx, fy):  # a sheet with a folded top-right corner and three lines of text
    x0, y0, x1, y1, fold = 3, 1, 13, 15, 4
    if not (x0 <= fx < x1 and y0 <= fy < y1):
        return None
    # the corner beyond the fold's diagonal is cut away; the diagonal itself is the fold line
    beyond = (fx - (x1 - fold)) - (fy - y0)  # > 0 past the diagonal from (x1 - fold, y0) to (x1, y0 + fold)
    if beyond > 0.7:
        return None
    outline = (0x8A, 0x8A, 0x8A)
    if beyond > -0.7 or fx < x0 + 1 or fx >= x1 - 1 or fy < y0 + 1 or fy >= y1 - 1:
        return outline
    if fx >= x1 - fold - 0.5 and fy < y0 + fold + 0.5:  # the folded-down flap: two inner edges and a grey face
        if fx < x1 - fold + 0.5 or fy >= y0 + fold - 0.5:
            return outline
        return (0xDC, 0xDC, 0xDC)
    if 5 <= fx < 11 and any(ly <= fy < ly + 1 for ly in (7, 9, 11)):
        return (0xA8, 0xA8, 0xA8)
    return WHITE


def add_icon(fx, fy):  # a purple plus
    d = min(abs(fx - 8) if 3 <= fy <= 13 else 9, abs(fy - 8) if 3 <= fx <= 13 else 9)
    return ACCENT if d <= 1.0 else None


def chevron(left):
    def f(fx, fy):
        tip, back = (5.5, 10.5) if left else (10.5, 5.5)
        d = min(seg_dist(fx, fy, back, 3.5, tip, 8), seg_dist(fx, fy, tip, 8, back, 12.5))
        return INK if d <= 1.0 else None

    return f


def dirty_icon(fx, fy):  # an accent dot: unsaved changes
    return ACCENT if (fx - 8) ** 2 + (fy - 8) ** 2 <= 3.6 ** 2 else None


for name, shape in [
    ("notebook", notebook_icon),
    ("section", section_icon),
    ("page", page_icon),
    ("add", add_icon),
    ("chevron-left", chevron(True)),
    ("chevron-right", chevron(False)),
    ("dirty", dirty_icon),
]:
    png(f"icons/{name}.png", 16, 16, smooth(shape))


# ---------------------------------------------------------------- ribbon command icons: 32 x 32, flat colours
# Original art, not copied from any product. Referenced from skin.conf as `ui.icon.cmd.<command> = image(icons/x.png)`;
# a large ribbon button draws them at 32 logical px, a small one at 16.
OUTLINE = (0x6B, 0x6B, 0x6B)
SHEET = (0xFF, 0xFF, 0xFF)
TEXTLINE = (0x9C, 0xB8, 0xDC)
BROWN = (0xB5, 0x7E, 0x3F)
BLUE = (0x2F, 0x6D, 0xB5)
GREEN = (0x3A, 0x9A, 0x48)
ORANGE = (0xF0, 0xA0, 0x2C)
RED = (0xD9, 0x45, 0x2E)


def sheet(fx, fy, x0, y0, x1, y1, lines=True):
    """A white sheet with a grey outline and blue text lines; None outside."""
    if not (x0 <= fx < x1 and y0 <= fy < y1):
        return None
    if fx < x0 + 1.2 or fx >= x1 - 1.2 or fy < y0 + 1.2 or fy >= y1 - 1.2:
        return OUTLINE
    if lines and x0 + 3 <= fx < x1 - 3 and any(ly <= fy < ly + 1.4 for ly in range(int(y0) + 5, int(y1) - 3, 4)):
        return TEXTLINE
    return SHEET


def paste_icon(fx, fy):  # a brown clipboard with a grey clip and a white sheet over its right half
    s = sheet(fx, fy, 13, 12, 29, 30)
    if s is not None:
        return s
    if in_round_rect(fx, fy, 10, 2, 19, 8, 1.5):
        return (0x70, 0x70, 0x70)
    if in_round_rect(fx, fy, 4, 5, 24, 29, 2.5):
        return (0x8A, 0x5C, 0x2A) if not in_round_rect(fx, fy, 5.3, 6.3, 22.7, 27.7, 1.5) else BROWN
    return None


def cut_icon(fx, fy):  # scissors: two blue finger rings and two crossing grey blades
    for cx in (10, 22):
        d = ((fx - cx) ** 2 + (fy - 24) ** 2) ** 0.5
        if 2.4 <= d <= 4.8:
            return BLUE
    if seg_dist(fx, fy, 12, 20.5, 21, 3) <= 1.4 or seg_dist(fx, fy, 20, 20.5, 11, 3) <= 1.4:
        return (0x7A, 0x7A, 0x7A)
    return None


def copy_icon(fx, fy):  # two sheets, the front one lower right
    s = sheet(fx, fy, 12, 10, 28, 30)
    if s is not None:
        return s
    return sheet(fx, fy, 4, 2, 20, 22)


def todo_icon(fx, fy):  # a checked box: green tick over a white box with a grey frame
    if seg_dist(fx, fy, 8, 16, 13.5, 22) <= 1.9 or seg_dist(fx, fy, 13.5, 22, 25, 7) <= 1.9:
        return GREEN
    if 5 <= fx < 27 and 5 <= fy < 27:
        inner = 6.5 <= fx < 25.5 and 6.5 <= fy < 25.5
        return SHEET if inner else OUTLINE
    return None


def tag_shape(fx, fy):  # an orange luggage tag pointing left, with a hole
    if not (2 <= fy < 16 and 2 <= fx < 20):
        return None
    if fx < 8 and abs(fy - 9) > (fx - 2):  # the pointed end
        return None
    if (fx - 7) ** 2 + (fy - 9) ** 2 <= 1.6 ** 2:
        return None
    return ORANGE


def find_tags_icon(fx, fy):  # a magnifier (blue ring, dark handle) over a tag
    d = ((fx - 18) ** 2 + (fy - 18) ** 2) ** 0.5
    if seg_dist(fx, fy, 23, 23, 29, 29) <= 2.0:
        return (0x45, 0x45, 0x45)
    if 6 <= d <= 8.2:
        return BLUE
    if d < 6:
        return (0xDD, 0xEB, 0xF8)
    return tag_shape(fx, fy)


def email_icon(fx, fy):  # an envelope with its flap drawn as a V
    if not (3 <= fx < 29 and 7 <= fy < 26):
        return None
    if fx < 4.3 or fx >= 27.7 or fy < 8.3 or fy >= 24.7:
        return BLUE
    if seg_dist(fx, fy, 4, 8.5, 16, 18) <= 0.9 or seg_dist(fx, fy, 28, 8.5, 16, 18) <= 0.9:
        return BLUE
    return SHEET


def meeting_icon(fx, fy):  # a calendar page: red header, two binder rings, a grid of days
    if (9 <= fx < 11 or 21 <= fx < 23) and 2 <= fy < 9:
        return (0x45, 0x45, 0x45)
    if not (4 <= fx < 28 and 5 <= fy < 28):
        return None
    if fy < 11:
        return RED
    if fx < 5.2 or fx >= 26.8 or fy >= 26.8:
        return OUTLINE
    if any(c <= fx < c + 4 for c in (7, 14, 21)) and any(r <= fy < r + 3 for r in (13, 18, 23)):
        return BLUE if (fx >= 14 and fx < 18 and 18 <= fy < 21) else (0xC8, 0xC8, 0xC8)
    return SHEET


def outlook_tasks_icon(fx, fy):  # a task list with a red flag on a pole at the right
    if 23 <= fx < 24.6 and 4 <= fy < 30:
        return (0x45, 0x45, 0x45)
    if 24.6 <= fx < 31 and 4 <= fy < 14 and abs(fy - 9) <= (31 - fx) * 0.8:
        return RED
    if not (3 <= fx < 21 and 4 <= fy < 29):
        return None
    if fx < 4.2 or fx >= 19.8 or fy < 5.2 or fy >= 27.8:
        return OUTLINE
    for ry in (8, 14, 20):  # a ticked box and a line per task
        if 6 <= fx < 10 and ry <= fy < ry + 4:
            return GREEN if ry == 8 else (0xA0, 0xA0, 0xA0)
        if 12 <= fx < 18 and ry + 1.3 <= fy < ry + 2.7:
            return TEXTLINE
    return SHEET


for name, shape in [
    ("paste", paste_icon),
    ("cut", cut_icon),
    ("copy", copy_icon),
    ("todo-tag", todo_icon),
    ("find-tags", find_tags_icon),
    ("email", email_icon),
    ("meeting", meeting_icon),
    ("outlook-tasks", outlook_tasks_icon),
]:
    png(f"icons/{name}.png", 32, 32, smooth(shape))
