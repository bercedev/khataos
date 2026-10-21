"""Build every KhataOS brand concept as hand-built SVG.

Run:  python3.11 branding/tools/build.py      (needs fontTools; see textpath.py for the font)
Then: node branding/tools/render.mjs          (renders the PNGs with headless Chromium)

All geometry is written by hand on a 64-unit grid (symbols) and a 16-unit pixel grid
(favicons). Text is outlined from Inter (SIL OFL 1.1), so no SVG depends on a font.
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from textpath import Text  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(HERE), "concepts")

# ---------------------------------------------------------------- palette
# Taken from apps/desktop/src/style.css so the brand and the app stay one system.
INDIGO = "#2b2f77"  # --brand / --topbar
INDIGO_2 = "#3f45a8"  # --brand-2
PERIWINKLE = "#8f96ff"  # --brand (dark)
PERIWINKLE_2 = "#a9aeff"  # --brand-2 (dark)
MARIGOLD = "#c76a12"  # --accent (light)
MARIGOLD_L = "#f2a65a"  # --accent (dark) / --topbar-key
PAPER = "#f5f3ee"  # --bg
PAPER_L = "#faf7f0"  # brand-mark cover
INK = "#1c1e2a"  # --ink
MUTED = "#676b7c"  # --muted
NIGHT = "#14151c"  # --bg (dark)
NIGHT_2 = "#1c1e28"  # --surface (dark)
NIGHT_IND = "#23264d"  # --topbar (dark)
MUTED_D = "#9b9eb1"  # --muted (dark)
WHITE = "#ffffff"


def n(v):
    s = ("%.2f" % v).rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def rrc(x, y, w, h, tl, tr=None, br=None, bl=None):
    """Rounded rectangle path with per-corner radii (clockwise)."""
    tr = tl if tr is None else tr
    br = tl if br is None else br
    bl = tl if bl is None else bl
    m = min(w, h) / 2
    tl, tr, br, bl = (min(r, m) for r in (tl, tr, br, bl))
    p = [f"M{n(x + tl)},{n(y)}H{n(x + w - tr)}"]
    if tr:
        p.append(f"A{n(tr)},{n(tr)} 0 0 1 {n(x + w)},{n(y + tr)}")
    p.append(f"V{n(y + h - br)}")
    if br:
        p.append(f"A{n(br)},{n(br)} 0 0 1 {n(x + w - br)},{n(y + h)}")
    p.append(f"H{n(x + bl)}")
    if bl:
        p.append(f"A{n(bl)},{n(bl)} 0 0 1 {n(x)},{n(y + h - bl)}")
    p.append(f"V{n(y + tl)}")
    if tl:
        p.append(f"A{n(tl)},{n(tl)} 0 0 1 {n(x + tl)},{n(y)}")
    p.append("Z")
    return "".join(p)


def rr(x, y, w, h, r=0):
    return rrc(x, y, w, h, r)


def poly(pts):
    return "M" + "L".join(f"{n(a)},{n(b)}" for a, b in pts) + "Z"


def squircle(cx, cy, size, k=5.0, steps=240):
    """Superellipse |x|^k + |y|^k = 1, close to Apple's continuous-corner icon shape."""
    a = size / 2
    pts = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        x = a * math.copysign(abs(c) ** (2 / k), c)
        y = a * math.copysign(abs(s) ** (2 / k), s)
        pts.append((cx + x, cy + y))
    return poly(pts)


def path(d, fill, extra=""):
    return f'<path d="{d}" fill="{fill}" fill-rule="evenodd"{extra}/>'


def tpath(d, fill):
    """Outlined text: variable-font glyphs overlap their own contours, so fill with nonzero."""
    return f'<path d="{d}" fill="{fill}"/>'


def svg(w, h, body, vb=None, title=None, desc=None):
    vb = vb or f"0 0 {n(w)} {n(h)}"
    t = f"<title>{title}</title>" if title else ""
    d = f"<desc>{desc}</desc>" if desc else ""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
        f'width="{n(w)}" height="{n(h)}" viewBox="{vb}">{t}{d}{body}</svg>\n'
    )


def write(rel, content):
    p = os.path.join(OUT, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w") as f:
        f.write(content)


# ---------------------------------------------------------------- themes
# Every mark is drawn for: light (on paper/white), dark (on night), tile (on its
# app-icon tile colour) and mono (one colour, details knocked out).


def theme_colours(concept, theme, mono=None):
    if theme == "mono":
        return dict(P=mono, P2=mono, A=mono, A_on=mono, mono=True)
    table = {
        "spine": {
            "light": dict(P=INDIGO, A=MARIGOLD),
            "dark": dict(P=PAPER_L, A=MARIGOLD_L),
            "tile": dict(P=PAPER_L, A=MARIGOLD_L),  # on the indigo tile, like today's top-bar mark
        },
        "ruledk": {
            "light": dict(P=INDIGO, A=MARIGOLD),
            "dark": dict(P=PAPER, A=MARIGOLD_L),
            "tile": dict(P=INDIGO, A=MARIGOLD),  # on the paper tile: blue rules, red margin
        },
        "key": {
            "light": dict(P=INDIGO, P2=INDIGO_2, A_on=MARIGOLD_L),
            "dark": dict(P=PERIWINKLE, P2=INDIGO, A_on=MARIGOLD_L),
            "tile": dict(P=INDIGO, P2=INDIGO_2, A_on=MARIGOLD_L),
        },
    }[concept][theme]
    c = dict(P2=table.get("P"), A=table.get("A"), A_on=table.get("A_on"), mono=False)
    c.update(table)
    return c


# ---------------------------------------------------------------- marks (64 grid)


def mark_spine(c):
    """Bound ledger: a wrapped spine standing proud of the cover, and a double totals rule."""
    spine = rr(9, 4, 12, 56, 4)
    cover = rrc(23, 8, 32, 48, 1, 4, 4, 1)
    rules = rr(29, 41, 20, 3, 0.5) + rr(29, 47, 20, 3, 0.5)
    return [path(spine, c["A"]), path(cover + rules, c["P"])]


def mark_spine_16(c):
    spine = rr(2, 1, 3, 14, 0.75)
    cover = rrc(6, 2, 8, 12, 0.25, 1, 1, 0.25)
    rules = rr(8, 9, 4, 1) + rr(8, 11, 4, 1)
    return [path(spine, c["A"]), path(cover + rules, c["P"])]


RK_ROWS = 5
RK_H = 7
RK_TOP = 6
RK_PITCH = 11.25
RK_STEP = 8
RK_LEN = 17
RK_X0 = 22


def mark_ruledk(c):
    """A K set from ledger rules: one margin rule (stem) and five ruled dashes on a chevron."""
    sh = RK_H * RK_STEP / RK_PITCH  # shear that lines dash ends up with the K's diagonals
    out = [path(rr(10, RK_TOP, 8, 52, 1.5), c["A"])]
    d = ""
    for i in range(RK_ROWS):
        y = RK_TOP + i * RK_PITCH
        k = abs(i - 2)
        x = RK_X0 + k * RK_STEP
        if k == 0:
            d += rr(x, y, RK_LEN, RK_H)
        elif i < 2:  # upper arm leans right as it rises
            d += poly([(x + sh / 2, y), (x + RK_LEN + sh / 2, y), (x + RK_LEN - sh / 2, y + RK_H), (x - sh / 2, y + RK_H)])
        else:  # lower arm leans right as it falls
            d += poly([(x - sh / 2, y), (x + RK_LEN - sh / 2, y), (x + RK_LEN + sh / 2, y + RK_H), (x + sh / 2, y + RK_H)])
    out.append(path(d, c["P"]))
    return out


def mark_ruledk_16(c):
    out = [path(rr(2, 1, 2, 14), c["A"])]
    d = ""
    xs = {0: 5, 1: 8, 2: 11}
    for i in range(5):
        x = xs[abs(i - 2)]
        d += rr(x, 1 + 3 * i, 4, 2)
    out.append(path(d, c["P"]))
    return out


def mark_key(c):
    """A keycap whose face carries '=': debits equal credits, and the app is driven by keys."""
    skirt = rr(6, 6, 52, 52, 11) + rr(10, 9, 44, 38, 9)
    face = rr(12, 11, 40, 34, 7)
    bars = rr(21, 20, 22, 5, 1) + rr(21, 31, 22, 5, 1)
    if c["mono"]:
        return [path(skirt, c["P"]), path(face + bars, c["P"])]
    return [path(skirt, c["P"]), path(face, c["P2"]), path(bars, c["A_on"])]


def mark_key_16(c):
    if c["mono"]:
        body = rr(1, 1, 14, 14, 3) + rr(5, 4, 6, 2) + rr(5, 7, 6, 2) + rr(3, 11, 10, 1)
        return [path(body, c["P"])]
    return [path(rr(1, 1, 14, 14, 3), c["P"]), path(rr(3, 2, 10, 9, 2), c["P2"]),
            path(rr(5, 4, 6, 2) + rr(5, 7, 6, 2), c["A_on"])]


# bbox of the ink in the 64 grid (x0, y0, x1, y1)
def _rk_bbox():
    sh = RK_H * RK_STEP / RK_PITCH
    return (10, RK_TOP, RK_X0 + 2 * RK_STEP + RK_LEN + sh / 2, RK_TOP + 52)


CONCEPTS = [
    dict(
        key="spine", dir="1-spine", name="Spine",
        mark=mark_spine, mark16=mark_spine_16, bbox=(9, 4, 55, 60),
        weight=600, os_weight=600, tracking=-0.012,
        tile=INDIGO, tile_theme="tile", hero="dark", hero_bg=INDIGO,
        android_bg=INDIGO,
        line="A bound ledger reduced to two shapes: the marigold spine and the indigo cover with a double totals rule.",
    ),
    dict(
        key="ruledk", dir="2-ruled-k", name="Ruled K",
        mark=mark_ruledk, mark16=mark_ruledk_16, bbox=_rk_bbox(),
        weight=700, os_weight=700, tracking=-0.02,
        tile=PAPER, tile_theme="tile", hero="light", hero_bg=PAPER,
        android_bg=PAPER,
        line="The K of Khata set from ledger rules: a marigold margin rule and five indigo rules stepping out like keystrokes.",
    ),
    dict(
        key="key", dir="3-balance-key", name="Balance Key",
        mark=mark_key, mark16=mark_key_16, bbox=(6, 6, 58, 58),
        weight=500, os_weight=700, tracking=-0.01,
        tile=INDIGO, tile_theme="tile", hero="dark", hero_bg=NIGHT,
        android_bg=PAPER,
        line="A single keycap with an equals sign on its face: debits equal credits, and every action is one key away.",
    ),
]


# ---------------------------------------------------------------- text

_texts = {}


def T(w):
    if w not in _texts:
        _texts[w] = Text(w)
    return _texts[w]


def wordmark(cfg, size, x, baseline, theme, mono=None):
    """Return (svg, ink_x0, ink_x1) for 'KhataOS' with 'OS' in the accent colour."""
    t1, t2 = T(cfg["weight"]), T(cfg["os_weight"])
    if theme == "mono":
        c1 = c2 = mono
    elif theme == "light":
        c1, c2 = INDIGO, MARIGOLD
    else:
        c1, c2 = PAPER, MARIGOLD_L
    tr = cfg["tracking"]
    d1, w1 = t1.path("Khata", size, x, baseline, tr)
    ka = t1.kern(t1.cmap[ord("a")], t1.cmap[ord("O")])
    x2 = x + w1 + (ka + tr * t1.upm) * size / t1.upm
    d2, w2 = t2.path("OS", size, x2, baseline, tr)
    # side bearings: ink of 'K' starts a little right of x, ink of 'S' ends a little left
    lsb = t1.font["hmtx"][t1.cmap[ord("K")]][1] * size / t1.upm
    gS = t2.cmap[ord("S")]
    s_adv, s_lsb = t2.font["hmtx"][gS]
    t2.font["glyf"][gS].recalcBounds(t2.font["glyf"])
    s_rsb = (s_adv - t2.font["glyf"][gS].xMax) * size / t2.upm
    body = tpath(d1, c1) + tpath(d2, c2)
    return body, x + lsb, x2 + w2 - s_rsb


def plain_text(s, size, x, baseline, fill, weight=400, anchor="start", tracking=0.0):
    t = T(weight)
    w = t.width(s, size, tracking)
    if anchor == "middle":
        x -= w / 2
    elif anchor == "end":
        x -= w
    d, _ = t.path(s, size, x, baseline, tracking)
    return tpath(d, fill)


def text_width(s, size, weight=400, tracking=0.0):
    return T(weight).width(s, size, tracking)


# ---------------------------------------------------------------- placement


def place_mark(cfg, colours, box, cx, cy, use16=False):
    """Mark scaled so its 64 grid is `box` px, with its ink centred on (cx, cy)."""
    if use16:
        s = box / 16
        body = "".join(cfg["mark16"](colours))
        return f'<g transform="translate({n(cx - 8 * s)},{n(cy - 8 * s)}) scale({n(s)})">{body}</g>'
    s = box / 64
    x0, y0, x1, y1 = cfg["bbox"]
    mx, my = (x0 + x1) / 2, (y0 + y1) / 2
    body = "".join(cfg["mark"](colours))
    tx, ty = n(cx - mx * s), n(cy - my * s)
    if s == 1:
        return body if tx == ty == "0" else f'<g transform="translate({tx},{ty})">{body}</g>'
    return f'<g transform="translate({tx},{ty}) scale({n(s)})">{body}</g>'


def mark_size(cfg, box):
    x0, y0, x1, y1 = cfg["bbox"]
    s = box / 64
    return (x1 - x0) * s, (y1 - y0) * s


def lockup_horizontal(cfg, theme, mono=None, box=128, x=0, cy=None, inner=False):
    """Symbol, then wordmark with its cap height centred on the symbol."""
    mw, mh = mark_size(cfg, box)
    size = box * 0.56
    cap = T(cfg["weight"]).cap * size
    gap = box * 0.2
    cy = mh / 2 if cy is None else cy
    colours = theme_colours(cfg["key"], theme, mono)
    g = place_mark(cfg, colours, box, x + mw / 2, cy)
    base = cy + cap / 2
    # start text so its ink begins `gap` after the symbol's ink
    probe, ix0, _ = wordmark(cfg, size, 0, 0, theme, mono)
    tx = x + mw + gap - ix0
    wm, _, ix1 = wordmark(cfg, size, tx, base, theme, mono)
    return g + wm, ix1 - x, mh


def lockup_stacked(cfg, theme, mono=None, box=160):
    mw, mh = mark_size(cfg, box)
    size = box * 0.34
    cap = T(cfg["weight"]).cap * size
    _, ix0, ix1 = wordmark(cfg, size, 0, 0, theme, mono)
    tw = ix1 - ix0
    W = max(mw, tw)
    colours = theme_colours(cfg["key"], theme, mono)
    g = place_mark(cfg, colours, box, W / 2, mh / 2)
    gap = box * 0.18
    base = mh + gap + cap
    wm, _, _ = wordmark(cfg, size, W / 2 - tw / 2 - ix0, base, theme, mono)
    return g + wm, W, base


# ---------------------------------------------------------------- backgrounds


def ledger_bg(w, h, theme, pitch=40, margin=None, top=None, bg=None):
    """Ledger paper: faint horizontal rules and a double marigold margin rule."""
    if theme == "light":
        bg = bg or PAPER
        rule, rule_op, mar, mar_op = INDIGO, 0.07, MARIGOLD, 0.55
    else:
        bg = bg or NIGHT
        rule, rule_op, mar, mar_op = PERIWINKLE, 0.08, MARIGOLD_L, 0.45
    out = [f'<rect width="{n(w)}" height="{n(h)}" fill="{bg}"/>']
    y = pitch if top is None else top
    d = ""
    while y < h:
        d += f"M0,{n(y)}H{n(w)}"
        y += pitch
    out.append(f'<path d="{d}" stroke="{rule}" stroke-opacity="{rule_op}" stroke-width="1.5" fill="none"/>')
    if margin is not None:
        out.append(
            f'<path d="M{n(margin)},0V{n(h)}M{n(margin + 6)},0V{n(h)}" stroke="{mar}" '
            f'stroke-opacity="{mar_op}" stroke-width="2" fill="none"/>'
        )
    return "".join(out)


TAG1 = "Open source accounting that works offline."
TAG2 = "Built for the keyboard. GST and VAT ready."
URL = "github.com/khataos/khataos"


def text_colours(theme):
    if theme == "light":
        return INK, MUTED
    return PAPER, "#c3c5dc" if theme == "indigo" else MUTED_D


# ---------------------------------------------------------------- builders


def build_concept(cfg):
    d = cfg["dir"]
    k = cfg["key"]
    title = f"KhataOS - {cfg['name']}"

    # symbols
    for theme, mono, fname in [
        ("light", None, "symbol.svg"),
        ("dark", None, "symbol-dark.svg"),
        ("mono", INK, "symbol-mono-ink.svg"),
        ("mono", WHITE, "symbol-mono-white.svg"),
        ("mono", INDIGO, "symbol-mono-indigo.svg"),
    ]:
        body = place_mark(cfg, theme_colours(k, theme, mono), 64, 32, 32)
        write(f"{d}/{fname}", svg(64, 64, body, title=title))

    # favicon: pixel-grid version for 16/32 px
    for theme, mono, fname in [("light", None, "favicon.svg"), ("dark", None, "favicon-dark.svg"),
                               ("mono", INK, "favicon-mono.svg")]:
        body = "".join(cfg["mark16"](theme_colours(k, theme, mono)))
        write(f"{d}/{fname}", svg(16, 16, body, title=title))
    # adaptive favicon: switches with the browser's colour scheme
    lb = "".join(cfg["mark16"](theme_colours(k, "light")))
    db = "".join(cfg["mark16"](theme_colours(k, "dark")))
    write(f"{d}/favicon-auto.svg", svg(16, 16,
        '<style>.d{display:none}@media (prefers-color-scheme:dark){.l{display:none}.d{display:inline}}</style>'
        f'<g class="l">{lb}</g><g class="d">{db}</g>', title=title))

    # wordmark
    for theme, mono, fname in [("light", None, "wordmark.svg"), ("dark", None, "wordmark-dark.svg"),
                               ("mono", INK, "wordmark-mono-ink.svg"), ("mono", WHITE, "wordmark-mono-white.svg")]:
        size = 72
        probe, ix0, ix1 = wordmark(cfg, size, 0, 0, theme, mono)
        top = max(-T(cfg["weight"]).bbox("Kh", size)[1], -T(cfg["os_weight"]).bbox("OS", size)[1])
        bot = max(T(cfg["weight"]).bbox("Kh", size)[3], T(cfg["os_weight"]).bbox("OS", size)[3])
        body, _, _ = wordmark(cfg, size, -ix0, top, theme, mono)
        write(f"{d}/{fname}", svg(ix1 - ix0, top + bot, body, title="KhataOS"))

    # lockups (tight to ink; clear space is documented in branding/README.md)
    for theme, mono, suffix in [("light", None, ""), ("dark", None, "-dark"), ("mono", INK, "-mono-ink"),
                                ("mono", WHITE, "-mono-white")]:
        body, W, H = lockup_horizontal(cfg, theme, mono)
        write(f"{d}/lockup-horizontal{suffix}.svg", svg(W, H, body, title=title))
        body, W, H = lockup_stacked(cfg, theme, mono)
        write(f"{d}/lockup-stacked{suffix}.svg", svg(W, H, body, title=title))

    build_icons(cfg)
    build_heroes(cfg)


def tile_mark(cfg, size, cx, cy, full=False):
    """What sits on the app-icon tile."""
    k = cfg["key"]
    if k == "key":
        # The tile itself is the keycap skirt; the face and '=' are scaled from the 64 grid.
        s = size / 52  # skirt 52 units wide in the mark
        x0, y0 = cx - size / 2, cy - size / 2
        fx, fy, fw, fh = 12 - 6, 11 - 6, 40, 34
        face = rr(x0 + (fx - 0.5) * s, y0 + (fy - 1) * s, (fw + 1) * s, (fh + 2) * s, 7.5 * s)
        bars = rr(x0 + 15 * s, y0 + 14 * s, 22 * s, 5 * s, 1 * s) + rr(x0 + 15 * s, y0 + 25 * s, 22 * s, 5 * s, 1 * s)
        return path(face, INDIGO_2) + path(bars, MARIGOLD_L)
    colours = theme_colours(k, cfg["tile_theme"])
    return place_mark(cfg, colours, size, cx, cy)


def build_icons(cfg):
    d, k = cfg["dir"], cfg["key"]
    shadow = (
        '<defs><filter id="s" x="-10%" y="-10%" width="120%" height="130%">'
        '<feDropShadow dx="0" dy="10" stdDeviation="12" flood-color="#000" flood-opacity="0.28"/></filter>'
        '<linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.10"/>'
        '<stop offset="0.5" stop-color="#fff" stop-opacity="0"/></linearGradient></defs>'
    )
    tile = squircle(512, 512, 824)
    if k == "key":
        inner = tile_mark(cfg, 824, 512, 512)
        mark_box = None
    else:
        inner = tile_mark(cfg, 600, 512, 512)
    body = (shadow + f'<g filter="url(#s)"><path d="{tile}" fill="{cfg["tile"]}"/></g>' + inner
            + f'<path d="{tile}" fill="url(#sheen)"/>')
    write(f"{d}/icon-1024.svg", svg(1024, 1024, body, title="KhataOS app icon"))

    # full bleed square (iOS / store listings / Windows tiles): the platform applies its own mask
    if k == "key":
        inner = tile_mark(cfg, 1024, 512, 512)
    else:
        inner = tile_mark(cfg, 720, 512, 512)
    write(f"{d}/icon-1024-fullbleed.svg",
          svg(1024, 1024, f'<rect width="1024" height="1024" fill="{cfg["tile"]}"/>' + inner, title="KhataOS app icon"))

    # Android adaptive layers: 108dp canvas, 72dp visible, 66dp safe circle (61% of the side)
    if k == "key":
        fg_col = theme_colours(k, "light")
    else:
        fg_col = theme_colours(k, cfg["tile_theme"])
    box = 430
    fg = place_mark(cfg, fg_col, box, 512, 512)
    mono = place_mark(cfg, theme_colours(k, "mono", WHITE), box, 512, 512)
    write(f"{d}/android/ic_launcher_foreground.svg", svg(1024, 1024, fg, title="KhataOS adaptive foreground"))
    write(f"{d}/android/ic_launcher_background.svg",
          svg(1024, 1024, f'<rect width="1024" height="1024" fill="{cfg["android_bg"]}"/>', title="KhataOS adaptive background"))
    write(f"{d}/android/ic_launcher_monochrome.svg", svg(1024, 1024, mono, title="KhataOS themed icon"))


def hero_block(cfg, theme, x, y, box, tag_size, align="start", W=None, url=True, tag2=True):
    """Lockup + tagline lines, left aligned at x (or centred on x)."""
    lt = "light" if theme == "light" else "dark"
    body, lw, lh = lockup_horizontal(cfg, lt, box=box)
    c1, c2 = text_colours(theme)
    ox = x if align == "start" else x - lw / 2
    out = f'<g transform="translate({n(ox)},{n(y)})">{body}</g>'
    ty = y + lh + tag_size * 1.9
    out += plain_text(TAG1, tag_size, x, ty, c1, weight=500, anchor="start" if align == "start" else "middle", tracking=-0.01)
    if tag2:
        ty += tag_size * 1.45
        out += plain_text(TAG2, tag_size * 0.72, x, ty, c2, weight=400, anchor="start" if align == "start" else "middle")
    return out, ty


def build_heroes(cfg):
    d = cfg["dir"]
    theme = cfg["hero"]
    bgtheme = "light" if theme == "light" else "dark"
    bg = cfg["hero_bg"]
    tag_theme = "indigo" if bg == INDIGO else theme
    url_col = text_colours(tag_theme)[1]

    # GitHub social preview 1280x640 (keep content inside a 40 px border)
    W, H = 1280, 640
    body = ledger_bg(W, H, bgtheme, pitch=40, margin=88, bg=bg)
    blk, ty = hero_block(cfg, tag_theme, 150, 176, 150, 42)
    body += blk
    body += plain_text(URL, 24, 150, 548, url_col, weight=500)
    write(f"{d}/social-preview.svg", svg(W, H, body, title="KhataOS social preview"))

    # Google Play feature graphic 1024x500 (no alpha; centred content survives cropping)
    W, H = 1024, 500
    body = ledger_bg(W, H, bgtheme, pitch=36, bg=bg)
    _, lw, lh = lockup_horizontal(cfg, "light", box=112)
    blk, _ = hero_block(cfg, tag_theme, W / 2, 172, 112, 34, align="middle", tag2=False)
    body += blk
    write(f"{d}/play-feature.svg", svg(W, H, body, title="KhataOS feature graphic"))

    # README banners 1600x400, light and dark
    for th, suffix in [("light", "light"), ("dark", "dark")]:
        W, H = 1600, 400
        bgc = PAPER if th == "light" else NIGHT
        body = ledger_bg(W, H, th, pitch=40, margin=72, bg=bgc)
        lt = th
        lk, lw, lh = lockup_horizontal(cfg, lt, box=128)
        body += f'<g transform="translate(132,{n(200 - lh / 2)})">{lk}</g>'
        c1, c2 = text_colours(th)
        body += plain_text(TAG1, 36, 1480, 192, c1, weight=500, anchor="end", tracking=-0.01)
        body += plain_text(TAG2, 25, 1480, 238, c2, weight=400, anchor="end")
        write(f"{d}/banner-{suffix}.svg", svg(W, H, body, title="KhataOS"))


# ---------------------------------------------------------------- AI hero variants


AI_LAYOUT = {
    # concept: (image, theme for text, text side, scrim colour)
    "spine": ("ai/hero-bg.jpg", "light", "right", PAPER),
    "ruledk": ("ai/hero-bg.jpg", "light", "right", PAPER),
    "key": ("ai/hero-bg.jpg", "dark", "right", NIGHT),
}


def build_ai_heroes(cfg):
    d, k = cfg["dir"], cfg["key"]
    img, theme, side, scrim = AI_LAYOUT[k]
    if not os.path.exists(os.path.join(OUT, d, img)):
        return
    lt = "light" if theme == "light" else "dark"
    for name, W, H, box, tag in [("social-preview-ai", 1280, 640, 124, 36),
                                 ("play-feature-ai", 1024, 500, 100, 30),
                                 ("banner-ai", 1600, 400, 112, 32)]:
        defs = (
            f'<defs><linearGradient id="g" x1="{0 if side == "right" else 1}" y1="0" x2="{1 if side == "right" else 0}" y2="0">'
            f'<stop offset="0.3" stop-color="{scrim}" stop-opacity="0"/>'
            f'<stop offset="0.6" stop-color="{scrim}" stop-opacity="0.8"/>'
            f'<stop offset="1" stop-color="{scrim}" stop-opacity="0.92"/></linearGradient></defs>'
        )
        body = defs + (f'<image href="{img}" x="0" y="0" width="{W}" height="{H}" preserveAspectRatio="xMidYMid slice"/>'
                       f'<rect width="{W}" height="{H}" fill="url(#g)"/>')
        # text starts right of the photographed object (left ~45% of every frame) and fits the frame
        x = 0.48 * W
        room = W * 0.94 - x
        tag = min(tag, tag * room / text_width(TAG1, tag, 500))
        _, lw, lh = lockup_horizontal(cfg, lt, box=box)
        if lw > room:
            box *= room / lw
            _, lw, lh = lockup_horizontal(cfg, lt, box=box)
        total = lh + tag * 1.9
        y = (H - total) / 2 - tag * 0.25
        blk, _ = hero_block(cfg, theme, x, y, box, tag, tag2=False)
        body += blk
        write(f"{d}/{name}.svg", svg(W, H, body, title="KhataOS"))


# ---------------------------------------------------------------- usage sheet (do / don't)


def build_usage(cfg):
    d, k = cfg["dir"], cfg["key"]
    TW, TH, GAP, LAB = 220, 170, 16, 34
    tiles = []

    def tile(bg, inner, label, ok, border=None):
        tiles.append((bg, inner, label, ok, border))

    L = lambda th, mono=None: place_mark(cfg, theme_colours(k, th, mono), 96, TW / 2, TH / 2)
    box = 96
    mw, mh = mark_size(cfg, box)
    cs = mh * 0.25
    clear = (f'<rect x="{n(TW / 2 - mw / 2 - cs)}" y="{n(TH / 2 - mh / 2 - cs)}" width="{n(mw + 2 * cs)}" '
             f'height="{n(mh + 2 * cs)}" fill="none" stroke="{MARIGOLD}" stroke-dasharray="4 3" stroke-width="1.5"/>')
    tile(PAPER, L("light"), "Full colour on light", True)
    tile(NIGHT, L("dark"), "Dark colourway on dark", True)
    tile(WHITE, L("mono", INK), "One colour, knocked out", True, "#e4e1d8")
    tile(PAPER, clear + L("light"), "Clear space: 1/4 height", True)
    tile(PAPER, f'<g transform="translate({TW / 2},{TH / 2}) scale(1.45,0.8) translate({-TW / 2},{-TH / 2})">{L("light")}</g>', "Stretch or squash", False)
    tile(PAPER, f'<g transform="rotate(-14 {TW / 2} {TH / 2})">{L("light")}</g>', "Rotate", False)
    recol = dict(P="#1b7a57", P2="#2f9a70", A="#b4232a", A_on="#b4232a", mono=False)
    tile(PAPER, place_mark(cfg, recol, 96, TW / 2, TH / 2), "Recolour", False)
    tile(PAPER, '<defs><filter id="ds"><feDropShadow dx="6" dy="8" stdDeviation="4" flood-opacity=".45"/></filter></defs>'
         f'<g filter="url(#ds)">{L("light")}</g>', "Add shadows or effects", False)
    tile(NIGHT, L("light"), "Light colourway on dark", False)
    busy = "".join(f'<rect x="{i * 22}" y="0" width="11" height="{TH}" fill="{c}"/>' for i, c in
                   zip(range(11), [MARIGOLD_L, PERIWINKLE, "#7a6326", MARIGOLD_L, "#237a4b", PERIWINKLE, MARIGOLD_L, "#b23b3b", PERIWINKLE, MARIGOLD_L, "#237a4b"]))
    tile(PAPER, busy + L("light"), "Place on busy patterns", False)

    cols = 5
    rows = (len(tiles) + cols - 1) // cols
    W = cols * TW + (cols - 1) * GAP
    H = rows * (TH + LAB) + (rows - 1) * GAP
    out = f'<rect width="{W}" height="{H}" fill="{WHITE}"/>'
    for i, (bg, inner, label, ok, border) in enumerate(tiles):
        x = (i % cols) * (TW + GAP)
        y = (i // cols) * (TH + LAB + GAP)
        stroke = f' stroke="{border}"' if border else ""
        out += (f'<g transform="translate({x},{y})"><clipPath id="c{i}"><rect width="{TW}" height="{TH}" rx="8"/></clipPath>'
                f'<g clip-path="url(#c{i})"><rect width="{TW}" height="{TH}" fill="{bg}"{stroke}/>{inner}</g>')
        mark = "Do" if ok else "Don't"
        col = "#1b7a57" if ok else "#b4232a"
        out += plain_text(mark, 13, 0, TH + 21, col, weight=700)
        out += plain_text(label, 13, text_width(mark, 13, 700) + 8, TH + 21, INK, weight=400)
        out += "</g>"
    write(f"{d}/usage.svg", svg(W, H, out, title="KhataOS logo usage"))


# ---------------------------------------------------------------- run

if __name__ == "__main__":
    for cfg in CONCEPTS:
        build_concept(cfg)
        build_ai_heroes(cfg)
        build_usage(cfg)
    # naming study: KhataOS vs Khata OS, in each concept's weights
    rows = ""
    y = 0
    for cfg in CONCEPTS:
        size = 56
        y += 92
        body, _, x1 = wordmark(cfg, size, 0, y, "light")
        rows += body
        t1, t2 = T(cfg["weight"]), T(cfg["os_weight"])
        x = 420
        d1, w1 = t1.path("Khata", size, x, y, cfg["tracking"])
        sp = t1.hmtx[t1.cmap[ord(" ")]][0] * size / t1.upm
        d2, _ = t2.path("OS", size, x + w1 + sp, y, cfg["tracking"])
        rows += tpath(d1, INDIGO) + tpath(d2, MARIGOLD)
    write("naming-study.svg", svg(820, y + 30, rows, title="KhataOS vs Khata OS"))
    print("built", OUT)
