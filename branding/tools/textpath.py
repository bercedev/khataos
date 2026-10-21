"""Outline text set in Inter into SVG path data (no font dependency in the output).

Font: Inter 3.019 variable (The Inter Project Authors), SIL Open Font License 1.1.
The OFL allows outlining glyphs into artwork such as logos; the outlines are not a
"Font Software" redistribution. See branding/README.md.

Usage (library): from textpath import Text; t = Text(600); d, width = t.path("KhataOS", size=48)
"""

import os
from functools import lru_cache

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

INTER = os.environ.get(
    "INTER_TTF",
    "/Applications/Adobe Premiere Pro 2026/Adobe Premiere Pro 2026.app/Contents/PlugIns/"
    "(AfterEffectsLib)/Effects/mochaAE/Resources/mochaui/Mocha AE.app/Contents/Resources/fonts/Inter.ttf",
)


@lru_cache(maxsize=None)
def _instance(weight):
    font = TTFont(INTER)
    return instancer.instantiateVariableFont(font, {"wght": weight, "slnt": 0})


def _pair_kerning(font):
    """Collect GPOS pair adjustments (XAdvance of the first glyph) into a lookup function."""
    gpos = font["GPOS"].table
    singles = {}
    classed = []
    for lookup in gpos.LookupList.Lookup:
        for sub in lookup.SubTable:
            if lookup.LookupType == 9:
                sub = sub.ExtSubTable
                if sub.LookupType != 2:
                    continue
            elif lookup.LookupType != 2:
                continue
            cov = sub.Coverage.glyphs
            if sub.Format == 1:
                for i, first in enumerate(cov):
                    for rec in sub.PairSet[i].PairValueRecord:
                        v = getattr(rec.Value1, "XAdvance", 0) if rec.Value1 else 0
                        if v:
                            singles.setdefault((first, rec.SecondGlyph), v)
            elif sub.Format == 2:
                classed.append((set(cov), sub))

    def kern(a, b):
        if (a, b) in singles:
            return singles[(a, b)]
        for cov, sub in classed:
            if a not in cov:
                continue
            c1 = sub.ClassDef1.classDefs.get(a, 0)
            c2 = sub.ClassDef2.classDefs.get(b, 0)
            rec = sub.Class1Record[c1].Class2Record[c2]
            v = getattr(rec.Value1, "XAdvance", 0) if rec.Value1 else 0
            if v:
                return v
        return 0

    return kern


class Text:
    def __init__(self, weight=600):
        self.font = _instance(weight)
        self.upm = self.font["head"].unitsPerEm
        self.cmap = self.font.getBestCmap()
        self.glyphs = self.font.getGlyphSet()
        self.hmtx = self.font["hmtx"]
        self.kern = _pair_kerning(self.font)
        os2 = self.font["OS/2"]
        self.cap = os2.sCapHeight / self.upm
        self.xh = os2.sxHeight / self.upm

    def path(self, text, size, x=0.0, y=0.0, tracking=0.0):
        """Return (svg path d, advance width) for text with baseline at (x, y).

        tracking is in em (e.g. -0.02). Kerning from GPOS pair positioning is applied.
        """
        s = size / self.upm
        pen = SVGPathPen(self.glyphs, ntos=lambda v: ("%.2f" % v).rstrip("0").rstrip("."))
        cursor = 0.0
        names = [self.cmap[ord(c)] for c in text]
        for i, name in enumerate(names):
            tp = TransformPen(pen, (s, 0, 0, -s, x + cursor * s, y))
            self.glyphs[name].draw(tp)
            adv = self.hmtx[name][0]
            if i + 1 < len(names):
                adv += self.kern(name, names[i + 1])
                adv += tracking * self.upm
            cursor += adv
        return pen.getCommands(), cursor * s

    def width(self, text, size, tracking=0.0):
        return self.path(text, size, tracking=tracking)[1]

    def bbox(self, text, size, x=0.0, y=0.0, tracking=0.0):
        """Ink bounds (x0, y0, x1, y1) in SVG coordinates (y down) for text at baseline (x, y)."""
        from fontTools.pens.boundsPen import BoundsPen

        s = size / self.upm
        bp = BoundsPen(self.glyphs)
        cursor = 0.0
        names = [self.cmap[ord(c)] for c in text]
        for i, name in enumerate(names):
            tp = TransformPen(bp, (s, 0, 0, -s, x + cursor * s, y))
            self.glyphs[name].draw(tp)
            adv = self.hmtx[name][0]
            if i + 1 < len(names):
                adv += self.kern(name, names[i + 1]) + tracking * self.upm
            cursor += adv
        x0, y0, x1, y1 = bp.bounds
        return x0, y0, x1, y1
