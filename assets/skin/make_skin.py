#!/usr/bin/env python3
"""Draws Waimea's default skin and packs it as src/skin/waimea.wsz.

All artwork is made here: no bitmap or glyph comes from another skin.
Usage: python3 assets/skin/make_skin.py  (needs Pillow; run from the repo root)
"""
import io
import subprocess
import zipfile

from PIL import Image, ImageDraw, ImageFont

SS = 6  # supersampling for smooth edges
INK = (6, 25, 47)  # displays; TEXT.BMP's background must match it
DEEP = (11, 37, 69)
LAGOON = (14, 90, 125)
SURF = (18, 164, 196)
FOAM = (127, 245, 227)
SPRAY = (228, 251, 247)
DIM = (84, 132, 158)
GLASS = (22, 72, 106)
GLASS_HI = (34, 104, 142)
HIBISCUS = (255, 122, 138)

GLYPHS = {
    "a": "0110 1001 1111 1001 1001", "b": "1110 1001 1110 1001 1110",
    "c": "0111 1000 1000 1000 0111", "d": "1110 1001 1001 1001 1110",
    "e": "1111 1000 1110 1000 1111", "f": "1111 1000 1110 1000 1000",
    "g": "0111 1000 1011 1001 0111", "h": "1001 1001 1111 1001 1001",
    "i": "1110 0100 0100 0100 1110", "j": "0011 0001 0001 1001 0110",
    "k": "1001 1010 1100 1010 1001", "l": "1000 1000 1000 1000 1111",
    "m": "1111 1011 1011 1001 1001", "n": "1001 1101 1111 1011 1001",
    "o": "0110 1001 1001 1001 0110", "p": "1110 1001 1110 1000 1000",
    "q": "0110 1001 1001 1011 0111", "r": "1110 1001 1110 1010 1001",
    "s": "0111 1000 0110 0001 1110", "t": "1110 0100 0100 0100 0100",
    "u": "1001 1001 1001 1001 0110", "v": "1001 1001 1001 0110 0110",
    "w": "1001 1001 1101 1101 1111", "x": "1001 1001 0110 1001 1001",
    "y": "1001 1001 0111 0001 0110", "z": "1111 0001 0110 1000 1111",
    '"': "1010 1010 0000 0000 0000", "@": "0110 1011 1011 1000 0111",
    "0": "0110 1011 1101 1001 0110", "1": "0100 1100 0100 0100 1110",
    "2": "1110 0001 0110 1000 1111", "3": "1110 0001 0110 0001 1110",
    "4": "1001 1001 1111 0001 0001", "5": "1111 1000 1110 0001 1110",
    "6": "0110 1000 1110 1001 0110", "7": "1111 0001 0010 0100 0100",
    "8": "0110 1001 0110 1001 0110", "9": "0110 1001 0111 0001 0110",
    "…": "0000 0000 0000 0000 1010", ".": "0000 0000 0000 0000 0100",
    ":": "0000 0100 0000 0100 0000", "(": "0010 0100 0100 0100 0010",
    ")": "0100 0010 0010 0010 0100", "-": "0000 0000 1110 0000 0000",
    "'": "0100 0100 0000 0000 0000", "!": "0100 0100 0100 0000 0100",
    "_": "0000 0000 0000 0000 1111", "+": "0000 0100 1110 0100 0000",
    "\\": "1000 0100 0100 0010 0001", "/": "0001 0010 0010 0100 1000",
    "[": "0110 0100 0100 0100 0110", "]": "0110 0010 0010 0010 0110",
    "^": "0100 1010 0000 0000 0000", "&": "0100 1010 0101 1010 0101",
    "%": "1001 0010 0100 1000 1001", ",": "0000 0000 0000 0100 1000",
    "=": "0000 1110 0000 1110 0000", "$": "0111 1010 0110 0101 1110",
    "#": "1010 1111 1010 1111 1010", "å": "0110 0110 1001 1111 1001",
    "ö": "1001 0110 1001 1001 0110", "ä": "1001 0110 1001 1111 1001",
    "?": "1110 0001 0110 0000 0100", "*": "1010 0100 1010 0000 0000",
}
FONT_ROWS = [
    list("abcdefghijklmnopqrstuvwxyz\"@"),
    list("0123456789….:()-'!_+\\/[]^&%,=$#"),
    list("åöä?*"),
]


def font_file(family):
    return subprocess.check_output(["fc-match", "-f", "%{file}", family], text=True)


DISPLAY_FONT = font_file("Ubuntu Sans:weight=80")


def display_font(px, axes=None):
    font = ImageFont.truetype(DISPLAY_FONT, px)
    if axes:
        try:
            font.set_variation_by_axes(axes)
        except OSError:
            pass
    return font


def gradient(w, h, top, bottom):
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(1, h - 1)
        d.line([(0, y), (w, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)))
    return img


class Hi:
    """Draw on an image at SS times its size, then shrink back."""

    def __init__(self, base):
        self.size = base.size
        self.img = base.convert("RGB").resize((base.width * SS, base.height * SS), Image.BICUBIC)
        self.d = ImageDraw.Draw(self.img, "RGBA")

    def s(self, *values):
        return [v * SS for v in values]

    def pill(self, x, y, w, h, fill=None, outline=None, r=None, width=1):
        r = h / 2 if r is None else r
        self.d.rounded_rectangle(self.s(x, y, x + w - 0.01, y + h - 0.01), radius=r * SS, fill=fill,
                                 outline=outline, width=round(width * SS))

    def poly(self, points, fill):
        self.d.polygon([(x * SS, y * SS) for x, y in points], fill=fill)

    def rect(self, x, y, w, h, fill):
        self.d.rectangle(self.s(x, y, x + w - 0.01, y + h - 0.01), fill=fill)

    def line(self, points, fill, width=1.0):
        self.d.line([(x * SS, y * SS) for x, y in points], fill=fill, width=round(width * SS), joint="curve")

    def circle(self, x, y, r, fill=None, outline=None, width=1):
        self.d.ellipse(self.s(x - r, y - r, x + r, y + r), fill=fill, outline=outline, width=round(width * SS))

    def text(self, xy, text, font, fill, anchor="mm", spacing=0):
        x, y = xy
        if not spacing:
            self.d.text((x * SS, y * SS), text, font=font, fill=fill, anchor=anchor)
            return
        widths = [font.getlength(c) / SS + spacing for c in text]
        x -= (sum(widths) - spacing) / 2
        for c, w in zip(text, widths):
            self.d.text((x * SS, y * SS), c, font=font, fill=fill, anchor="lm")
            x += w

    def done(self):
        return self.img.resize(self.size, Image.LANCZOS)


def wave_points(x0, x1, y, amp, step=1.0, phase=0.0, period=60.0):
    import math
    points, x = [], x0
    while x <= x1:
        points.append((x, y + amp * math.sin((x + phase) / period * 2 * math.pi)))
        x += step
    return points


def wave_logo(hi, x, y, size, fill):
    # The app icon's wave, from its 64 unit grid.
    k = size / 64
    def bezier(p0, p1, p2, p3):
        out = []
        for i in range(1, 25):
            t = i / 24
            out.append(tuple((1 - t) ** 3 * a + 3 * (1 - t) ** 2 * t * b + 3 * (1 - t) * t * t * c + t ** 3 * d
                             for a, b, c, d in zip(p0, p1, p2, p3)))
        return out
    path = [(0, 50)]
    segments = [((0, 50), (8, 48), (14, 42), (19, 33)), ((19, 33), (25, 21), (34, 12), (45, 12)),
                ((45, 12), (54, 12), (60, 19), (59, 27)), ((59, 27), (58, 34), (50, 37), (45, 33)),
                ((45, 33), (41, 30), (42, 24), (47, 24)), ((47, 24), (43, 21), (37, 24), (34, 31)),
                ((34, 31), (30, 41), (36, 50), (64, 48))]
    for seg in segments:
        path += bezier(*seg)
    path.append((64, 56))
    path.append((0, 56))
    hi.poly([(x + a * k, y + b * k) for a, b in path], fill)


def pixel_text(img, xy, text, color):
    x, y = xy
    px = img.load()
    for ch in text.lower():
        rows = GLYPHS.get(ch)
        if rows:
            for ry, row in enumerate(rows.split()):
                for rx, bit in enumerate(row):
                    if bit == "1":
                        px[x + rx, y + ry] = color
        x += 5


def text_w(text):
    return len(text) * 5 - 1


def sheet(w, h, color=DEEP):
    return Image.new("RGB", (w, h), color)


def paste_drawn(target, at, bg, draw):
    hi = Hi(bg)
    draw(hi)
    target.paste(hi.done(), at)


# ---------------------------------------------------------------- main window
def make_main():
    bg = gradient(275, 116, (17, 56, 92), (10, 44, 78))
    hi = Hi(bg)
    # The swell the player floats on.
    for i, (amp, y, color) in enumerate([(5, 100, (18, 164, 196, 40)), (4, 107, (127, 245, 227, 22))]):
        hi.poly(wave_points(0, 275, y, amp, phase=i * 20, period=120) + [(275, 116), (0, 116)], color)
    hi.pill(14, 40, 93, 23, fill=INK, r=5)
    hi.pill(8, 17, 263, 38, fill=INK, r=6)
    # Glass rails the buttons sit on.
    hi.pill(14, 86, 118, 22, fill=(255, 255, 255, 16))
    hi.pill(14, 70, 252, 14, fill=(0, 0, 0, 34), r=7)
    wave_logo(hi, 240, 84, 28, (*FOAM, 235))
    img = hi.done()
    # Flat ink again where opaque sprites land inside the display.
    flat = ImageDraw.Draw(img)
    flat.rectangle((20, 24, 268, 53), fill=INK)
    flat.rectangle((22, 43, 102, 60), fill=INK)
    flat.rectangle((72, 29, 73, 30), fill=DIM)
    flat.rectangle((72, 34, 73, 35), fill=DIM)
    pixel_text(img, (127, 43), "kbps", DIM)
    pixel_text(img, (167, 43), "khz", DIM)
    return img


def make_title(main):
    s = sheet(344, 87)
    def bar(active):
        hi = Hi(gradient(275, 14, (24, 72, 110) if active else (16, 46, 76), (17, 56, 92)))
        hi.text((137.5, 7.2), "WAIMEA", display_font(8 * SS, [700]), SPRAY if active else DIM, spacing=1.6)
        color = SPRAY if active else DIM
        # Minimize and close; the other two title slots do nothing here.
        hi.line([(246.5, 8.5), (251, 8.5)], color, 1.1)
        hi.line([(266.5, 5.5), (271, 10)], color, 1.1)
        hi.line([(271, 5.5), (266.5, 10)], color, 1.1)
        return hi.done()
    s.paste(bar(True), (27, 0))
    s.paste(bar(False), (27, 15))
    active = bar(True)
    for sprite, x, draw in [((0, 9), 6, None), ((9, 18), 254, None), ((9, 9), 244, "min"), ((18, 9), 264, "close")]:
        crop = active.crop((x, 3, x + 9, 12))
        if draw:
            def paint(hi, kind=draw):
                hi.circle(4.5, 4.5, 4.5, fill=HIBISCUS if kind == "close" else GLASS_HI)
                if kind == "close":
                    hi.line([(2.5, 2.5), (6.5, 6.5)], INK, 1.1)
                    hi.line([(6.5, 2.5), (2.5, 6.5)], INK, 1.1)
                else:
                    hi.line([(2.5, 5.5), (6.5, 5.5)], SPRAY, 1.1)
            paste_drawn(s, sprite, crop, paint)
        else:
            s.paste(crop, sprite)
    s.paste(main.crop((10, 22, 18, 65)), (304, 0))
    return s


def transport_icon(hi, kind, w, h, color):
    cx, cy = w / 2, h / 2
    if kind == "prev":
        hi.poly([(cx + 1, cy - 4), (cx + 1, cy + 4), (cx - 5, cy)], color)
        hi.rect(cx + 2.5, cy - 4, 1.6, 8, color)
    elif kind == "next":
        hi.poly([(cx - 1, cy - 4), (cx - 1, cy + 4), (cx + 5, cy)], color)
        hi.rect(cx - 4.1, cy - 4, 1.6, 8, color)
    elif kind == "play":
        hi.poly([(cx - 3.5, cy - 5.5), (cx - 3.5, cy + 5.5), (cx + 6, cy)], color)
    elif kind == "pause":
        hi.pill(cx - 4, cy - 4.5, 3, 9, fill=color, r=0.8)
        hi.pill(cx + 1, cy - 4.5, 3, 9, fill=color, r=0.8)
    elif kind == "stop":
        hi.pill(cx - 4, cy - 4, 8, 8, fill=color, r=1.4)
    elif kind == "eject":
        hi.poly([(cx, cy - 5), (cx + 5, cy + 0.5), (cx - 5, cy + 0.5)], color)
        hi.pill(cx - 5, cy + 2, 10, 2, fill=color, r=0.8)


def make_cbuttons(main):
    s = sheet(136, 36)
    items = [("prev", 16, 0, 23), ("play", 39, 23, 23), ("pause", 62, 46, 23), ("stop", 85, 69, 23), ("next", 108, 92, 22)]
    for kind, wx, sx, w in items:
        crop = main.crop((wx, 88, wx + w, 106))
        for row, pressed in ((0, False), (18, True)):
            def paint(hi):
                if pressed:
                    hi.pill(1, 0.5, w - 2, 17, fill=(255, 255, 255, 46))
                transport_icon(hi, kind, w, 18, FOAM if kind == "play" or pressed else SPRAY)
            paste_drawn(s, (sx, row), crop, paint)
    crop = main.crop((136, 89, 158, 105))
    for row, pressed in ((0, False), (16, True)):
        def paint(hi):
            hi.pill(0.5, 0.5, 21, 15, fill=(255, 255, 255, 60 if pressed else 22))
            transport_icon(hi, "eject", 22, 16, FOAM if pressed else SPRAY)
        paste_drawn(s, (114, row), crop, paint)
    return s


def toggle(hi, w, h, on, down):
    fill = (*FOAM, 70 if down else 42) if on else (255, 255, 255, 56 if down else 20)
    hi.pill(0.5, 0.5, w - 1, h - 1, fill=fill, outline=(*FOAM, 200) if on else None, width=0.6)
    return FOAM if on else SPRAY


def make_shufrep(main):
    s = sheet(92, 85)
    for row, on, down in [(0, False, False), (15, False, True), (30, True, False), (45, True, True)]:
        def shuffle(hi):
            c = toggle(hi, 45, 15, on, down)
            cx = 22.5
            hi.line([(cx - 9, 5), (cx - 4, 5), (cx + 2, 10), (cx + 7, 10)], c, 1.1)
            hi.line([(cx - 9, 10), (cx - 4, 10), (cx + 2, 5), (cx + 7, 5)], c, 1.1)
            for y in (5, 10):
                hi.poly([(cx + 6.5, y - 2.3), (cx + 10, y), (cx + 6.5, y + 2.3)], c)
        paste_drawn(s, (28, row), main.crop((164, 89, 209, 104)), shuffle)
        s.paste(main.crop((209, 89, 211, 104)), (73, row))
        def repeat(hi):
            c = toggle(hi, 26, 15, on, down)
            hi.pill(7, 4, 12, 7, outline=c, r=3, width=1.1)
            hi.poly([(11, 1.8), (14.5, 4), (11, 6.2)], c)
        paste_drawn(s, (0, row), main.crop((210, 89, 236, 104)), repeat)
        s.paste(main.crop((236, 89, 238, 104)), (26, row))
    for dy, on in ((61, False), (73, True)):
        for dx, down in ((0, False), (46, True)):
            for name, offset, wx in (("eq", 0, 219), ("pl", 23, 242)):
                def paint(hi):
                    toggle(hi, 22, 12, on, down)
                crop = main.crop((wx, 58, wx + 23, 70))
                hi = Hi(crop)
                paint(hi)
                img = hi.done()
                pixel_text(img, (6 if name == "pl" else 6, 3), name, FOAM if on else SPRAY)
                s.paste(img, (dx + offset, dy))
    return s


def make_digits(width):
    s = sheet(width, 13, INK)
    font = display_font(16 * SS, [300])
    for i, digit in enumerate("0123456789"):
        hi = Hi(sheet(9, 13, INK))
        hi.d.text((4.5 * SS, 6.5 * SS), digit, font=font, fill=FOAM, anchor="mm")
        s.paste(hi.done(), (i * 9, 0))
    if width == 108:
        hi = Hi(sheet(9, 13, INK))
        hi.line([(1.5, 6.5), (7.5, 6.5)], FOAM, 1.4)
        s.paste(hi.done(), (99, 0))
    return s


def make_text():
    s = sheet(155, 18, INK)
    for r, row in enumerate(FONT_ROWS):
        for c, ch in enumerate(row):
            pixel_text(s, (c * 5, r * 6), ch, SPRAY)
    return s


def slider_sheet(main, w_full, at, frame_x, width, fill_of, thumb_x):
    s = sheet(w_full, 433)
    crop = main.crop((at[0], at[1], at[0] + width, at[1] + 13))
    for i in range(28):
        def paint(hi):
            hi.pill(1, 5, width - 2, 3, fill=(255, 255, 255, 34))
            fill = fill_of(i)
            if fill:
                hi.pill(1, 5, fill, 3, fill=FOAM)
        paste_drawn(s, (frame_x, i * 15), crop, paint)
    hi = Hi(crop)
    hi.pill(0, 5, width, 3, fill=(255, 255, 255, 34), r=0)
    plain = hi.done().crop((10, 1, 24, 12))
    for x, color in ((15, SPRAY), (0, FOAM)):
        def thumb(hi):
            hi.circle(7, 5.5, 4.6, fill=color)
            hi.circle(7, 5.5, 1.4, fill=(*DEEP, 120))
        paste_drawn(s, (x, 422), plain, thumb)
    return s


def make_volume(main):
    return slider_sheet(main, 68, (107, 57), 0, 68, lambda i: max(3, 7 + i / 27 * 51), 0)


def make_balance(main):
    return slider_sheet(main, 47, (177, 57), 9, 38, lambda i: None, 0)


def make_posbar(main):
    s = sheet(307, 10)
    crop = main.crop((16, 72, 264, 82))
    def track(hi):
        hi.pill(1, 3.5, 246, 3, fill=(255, 255, 255, 40))
    paste_drawn(s, (0, 0), crop, track)
    plain = s.crop((100, 0, 129, 10))
    for x, color in ((248, FOAM), (278, SPRAY)):
        def thumb(hi):
            hi.pill(3, 1.5, 23, 7, fill=color)
        paste_drawn(s, (x, 0), plain, thumb)
    return s


def make_playpaus():
    s = sheet(42, 9, INK)
    hi = Hi(s)
    hi.poly([(2, 1), (2, 8), (8, 4.5)], FOAM)
    hi.rect(10.5, 1, 2, 7, SPRAY)
    hi.rect(14.5, 1, 2, 7, SPRAY)
    hi.pill(20, 1.5, 6, 6, fill=DIM, r=1)
    hi.circle(37.5, 4.5, 1.2, fill=FOAM)
    return hi.done()


def make_monoster():
    s = sheet(58, 24, INK)
    pixel_text(s, (0, 3), "stereo", FOAM)
    pixel_text(s, (0, 15), "stereo", DIM)
    pixel_text(s, (33, 3), "mono", FOAM)
    pixel_text(s, (33, 15), "mono", DIM)
    return s


# ------------------------------------------------------------------ equalizer
def make_eqmain():
    s = sheet(275, 315)
    hi = Hi(gradient(275, 116, (14, 48, 82), (10, 44, 78)))
    hi.poly(wave_points(0, 275, 106, 4, period=120) + [(275, 116), (0, 116)], (18, 164, 196, 34))
    bg = hi.done()
    labels = ["60", "170", "310", "600", "1k", "3k", "6k", "12k", "14k", "16k"]
    for i, label in enumerate(labels):
        pixel_text(bg, (85 - text_w(label) // 2 + i * 18, 104), label, DIM)
    pixel_text(bg, (14, 104), "pre", DIM)
    pixel_text(bg, (40, 38), "+12", DIM)
    pixel_text(bg, (50, 67), "0", DIM)
    pixel_text(bg, (40, 96), "-12", DIM)
    s.paste(bg, (0, 0))

    def title(active):
        hi = Hi(gradient(275, 14, (24, 72, 110) if active else (16, 46, 76), (14, 48, 82)))
        hi.text((137.5, 7.2), "EQUALIZER", display_font(8 * SS, [700]), SPRAY if active else DIM, spacing=1.4)
        c = SPRAY if active else DIM
        hi.line([(266.5, 5.5), (271, 10)], c, 1.1)
        hi.line([(271, 5.5), (266.5, 10)], c, 1.1)
        return hi.done()
    s.paste(title(True), (0, 134))
    s.paste(title(False), (0, 149))
    close = bg.crop((264, 3, 273, 12))
    def close_on(hi):
        hi.circle(4.5, 4.5, 4.5, fill=HIBISCUS)
        hi.line([(2.5, 2.5), (6.5, 6.5)], INK, 1.1)
        hi.line([(6.5, 2.5), (2.5, 6.5)], INK, 1.1)
    paste_drawn(s, (0, 125), close, close_on)

    def eq_toggle(x, y, w, text, tx, on, down, at):
        hi = Hi(bg.crop((at, 18, at + w, 30)))
        toggle(hi, w, 12, on, down)
        img = hi.done()
        pixel_text(img, (tx, 3), text, FOAM if on else SPRAY)
        s.paste(img, (x, y))
    for x, on, down in ((10, False, False), (69, True, False), (128, False, True), (187, True, True)):
        eq_toggle(x, 119, 26, "on", 8, on, down, 14)
    for x, on, down in ((36, False, False), (95, True, False), (154, False, True), (213, True, True)):
        eq_toggle(x, 119, 32, "auto", 6, on, down, 40)

    band = bg.crop((78, 38, 92, 101))
    for n in range(28):
        def paint(hi):
            hi.pill(5.5, 1, 3, 61, fill=(255, 255, 255, 34))
            v = n / 27
            y = (1 - v) * 51 + 5.5
            hi.pill(5.5, y, 3, 62 - y, fill=(*FOAM, 220))
        paste_drawn(s, (13 + (n % 14) * 15, 164 + (n // 14) * 65), band, paint)
    for y, color in ((164, SPRAY), (176, FOAM)):
        def thumb(hi):
            hi.circle(5.5, 5.5, 4.8, fill=color)
            hi.circle(5.5, 5.5, 1.4, fill=(*DEEP, 120))
        paste_drawn(s, (0, y), band.crop((1, 20, 12, 31)), thumb)
    for y, down in ((164, False), (176, True)):
        hi = Hi(bg.crop((217, 18, 261, 30)))
        toggle(hi, 44, 12, False, down)
        img = hi.done()
        pixel_text(img, (8, 3), "preset", SPRAY)
        s.paste(img, (224, y))
    graph = Hi(sheet(113, 19, INK))
    graph.line([(2, 9.5), (111, 9.5)], (255, 255, 255, 30), 0.6)
    s.paste(graph.done(), (0, 294))
    for i in range(19):
        t = i / 18
        s.putpixel((115, 294 + i), tuple(round(a + (b - a) * t) for a, b in zip(FOAM, SURF)))
    for x in range(113):
        s.putpixel((x, 314), DIM)
    return s


# ------------------------------------------------------------------- playlist
def make_pledit():
    s = sheet(280, 186)
    def top(y, active):
        bar = gradient(179, 20, (24, 72, 110) if active else (16, 46, 76), (14, 48, 82))
        s.paste(bar.crop((0, 0, 25, 20)), (0, y))
        s.paste(bar.crop((0, 0, 25, 20)), (127, y))
        hi = Hi(bar.crop((0, 0, 100, 20)))
        hi.text((50, 8.5), "PLAYLIST", display_font(8 * SS, [700]), SPRAY if active else DIM, spacing=1.4)
        s.paste(hi.done(), (26, y))
        hi = Hi(bar.crop((0, 0, 25, 20)))
        c = SPRAY if active else DIM
        hi.line([(16.5, 5.5), (21, 10)], c, 1.1)
        hi.line([(21, 5.5), (16.5, 10)], c, 1.1)
        s.paste(hi.done(), (153, y))
    top(0, True)
    top(21, False)
    edge = gradient(51, 29, (14, 48, 82), (14, 48, 82))
    s.paste(edge.crop((0, 0, 12, 29)), (0, 42))
    hi = Hi(edge.crop((0, 0, 20, 29)))
    hi.pill(8, 0, 4, 29, fill=(255, 255, 255, 16), r=0)
    s.paste(hi.done(), (31, 42))
    for x, color in ((52, GLASS_HI), (61, FOAM)):
        hi = Hi(edge.crop((0, 0, 8, 18)))
        hi.pill(0.5, 0.5, 7, 17, fill=color, r=3.5)
        s.paste(hi.done(), (x, 53))
    hi = Hi(edge.crop((0, 0, 9, 9)))
    close_on = lambda h: (h.circle(4.5, 4.5, 4.5, fill=HIBISCUS), h.line([(2.5, 2.5), (6.5, 6.5)], INK, 1.1), h.line([(6.5, 2.5), (2.5, 6.5)], INK, 1.1))
    close_on(hi)
    s.paste(hi.done(), (52, 42))

    foot = gradient(150, 38, (14, 48, 82), (10, 44, 78))
    s.paste(foot.crop((0, 0, 25, 38)), (179, 0))
    hi = Hi(foot.crop((0, 0, 125, 38)))
    hi.pill(14, 8, 22, 18, fill=(255, 255, 255, 22))
    hi.line([(25, 13), (25, 21)], FOAM, 1.4)
    hi.line([(21, 17), (29, 17)], FOAM, 1.4)
    s.paste(hi.done(), (0, 72))
    hi = Hi(foot)
    hi.pill(3, 7, 90, 12, fill=INK, r=3)
    hi.pill(3, 20, 88, 13, fill=INK, r=3)
    for i, kind in enumerate(["prev", "play", "pause", "stop", "next", "eject"]):
        small = Hi(sheet(10, 10, INK))
        hi_icon = {"prev": [(7, 2), (7, 8), (3, 5)], "play": [(3, 2), (3, 8), (8, 5)], "next": [(3, 2), (3, 8), (7, 5)], "eject": [(5, 2), (8, 6), (2, 6)]}
        if kind in hi_icon:
            hi.poly([(3 + i * 10 + a, 22 + b) for a, b in hi_icon[kind]], SPRAY)
        elif kind == "pause":
            hi.rect(3 + i * 10 + 3, 24, 1.4, 6, SPRAY)
            hi.rect(3 + i * 10 + 6, 24, 1.4, 6, SPRAY)
        else:
            hi.rect(3 + i * 10 + 2.5, 24.5, 5, 5, SPRAY)
    wave_logo(hi, 118, 6, 26, (*FOAM, 235))
    img = hi.done()
    s.paste(img, (126, 72))
    return s


def main_build():
    main = make_main()
    files = {
        "MAIN.BMP": main,
        "TITLEBAR.BMP": make_title(main),
        "CBUTTONS.BMP": make_cbuttons(main),
        "SHUFREP.BMP": make_shufrep(main),
        "TEXT.BMP": make_text(),
        "NUMBERS.BMP": make_digits(99),
        "NUMS_EX.BMP": make_digits(108),
        "VOLUME.BMP": make_volume(main),
        "BALANCE.BMP": make_balance(main),
        "POSBAR.BMP": make_posbar(main),
        "PLAYPAUS.BMP": make_playpaus(),
        "MONOSTER.BMP": make_monoster(),
        "EQMAIN.BMP": make_eqmain(),
        "PLEDIT.BMP": make_pledit(),
    }
    hexa = lambda c: "#%02X%02X%02X" % c
    pledit = f"[Text]\r\nNormal={hexa((198, 232, 236))}\r\nCurrent={hexa(FOAM)}\r\nNormalBG={hexa(INK)}\r\nSelectedBG={hexa((16, 62, 96))}\r\nFont=Ubuntu\r\n"
    colors = [INK, INK]
    for i in range(16):
        t = i / 15
        colors.append(tuple(round(a + (b - a) * t) for a, b in zip(FOAM, SURF)))
    colors += [FOAM, FOAM, SPRAY, FOAM, FOAM, HIBISCUS]
    viscolor = "".join("%d,%d,%d,\r\n" % c for c in colors)
    readme = "Waimea\r\nDeep sea glass, the default skin of Waimea.\r\nLicense: MIT\r\n"
    with zipfile.ZipFile("src/skin/waimea.wsz", "w", zipfile.ZIP_DEFLATED) as z:
        for name, img in files.items():
            buf = io.BytesIO()
            img.convert("RGB").save(buf, "BMP")
            z.writestr(zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0)), buf.getvalue(), zipfile.ZIP_DEFLATED)
        for name, text in (("PLEDIT.TXT", pledit), ("VISCOLOR.TXT", viscolor), ("README.TXT", readme)):
            z.writestr(zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0)), text, zipfile.ZIP_DEFLATED)


if __name__ == "__main__":
    main_build()
