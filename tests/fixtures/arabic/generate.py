# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Regenerate the original synthetic Arabic font using only Python's standard library.

No outlines, anchors or feature code are copied from an external font. The
deliberately simple polygons exercise joining forms, a lam-alef ligature,
marks on bases, ligatures and other marks, cursive attachment, right-to-left
kerning and two unfinished-master cases. README.md lists the test strings.
Run: python3 tests/fixtures/arabic/generate.py
"""

from pathlib import Path
import plistlib
import shutil
from xml.etree import ElementTree as ET

ROOT = Path(__file__).resolve().parent
FAMILY = "Runebender Arabic Fixture"

# Glyph name, code point, OpenType category.
LATIN = [
    (".notdef", None), ("space", 0x20), ("period", 0x2E),
    ("zero", 0x30), ("one", 0x31), ("two", 0x32),
    ("A", 0x41), ("T", 0x54), ("V", 0x56), ("a", 0x61), ("n", 0x6E), ("o", 0x6F),
]
DUAL = [("beh-ar", 0x628), ("seen-ar", 0x633), ("lam-ar", 0x644),
        ("meem-ar", 0x645), ("heh-ar", 0x647), ("yeh-ar", 0x64A)]
RIGHT = [("alef-ar", 0x627), ("reh-ar", 0x631)]
MARKS = [("fatha-ar", 0x64E), ("damma-ar", 0x64F), ("kasra-ar", 0x650),
         ("shadda-ar", 0x651), ("sukun-ar", 0x652), ("hamzaabove-ar", 0x654),
         ("alefabove-ar", 0x670)]
# Unfinished-master glyphs. No test string and no feature rule uses them.
INCOMPATIBLE = ("noon-ar", 0x646)
PARTIAL_ANCHOR = ("dal-ar", 0x62F)


def glyph_list():
    glyphs = list(LATIN)
    for name, codepoint in RIGHT:
        glyphs += [(name, codepoint), (f"{name}.fina", None)]
    glyphs.append(("alefHamzaabove-ar", 0x623))
    for name, codepoint in DUAL:
        glyphs.append((name, codepoint))
        glyphs += [(f"{name}.{form}", None) for form in ("init", "medi", "fina")]
    glyphs += [("lam_alef-ar", None), ("lam_alef-ar.fina", None)]
    glyphs += MARKS
    glyphs += [INCOMPATIBLE, PARTIAL_ANCHOR]
    return glyphs


def plist(path, value):
    path.write_bytes(plistlib.dumps(value, sort_keys=True))


def rectangle(x0, y0, x1, y1):
    return [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]


def ring(cx, cy, rx, ry, t):
    """A square ring as an outer contour and a reversed counter."""
    outer = rectangle(cx - rx, cy - ry, cx + rx, cy + ry)
    inner = rectangle(cx - rx + t, cy - ry + t, cx + rx - t, cy + ry - t)
    return [outer, list(reversed(inner))]


def comb(left, right, t, teeth):
    """A baseline stroke from `left` to `right` with vertical teeth of (center, height)."""
    points = [(left, 0), (left, t)]
    for center, height in sorted(teeth):
        half = t // 2
        points += [(center - half, t), (center - half, height),
                   (center + half, height), (center + half, t)]
    return points + [(right, t), (right, 0)]


def ends(form, width):
    """Stroke ends: a joined side reaches the advance edge, a free side keeps 60 units."""
    joins_left = form in ("init", "medi")
    joins_right = form in ("medi", "fina")
    return (0 if joins_left else 60), (width if joins_right else width - 60)


class Drawing:
    def __init__(self, width):
        self.width = width
        self.contours = []
        self.curves = []
        self.anchors = []

    def anchor(self, name, x, y):
        self.anchors.append((name, round(x), round(y)))


def dot(drawing, cx, cy, t):
    drawing.contours.append(rectangle(cx - t // 2, cy - t // 2, cx + t // 2, cy + t // 2))


def dual_letter(base, form, t):
    """Outlines and anchors for one form of a dual-joining letter."""
    g = t - 80
    widths = {
        "beh-ar": {"isol": 520, "init": 300, "medi": 240, "fina": 480},
        "seen-ar": {"isol": 640, "init": 440, "medi": 400, "fina": 600},
        "lam-ar": {"isol": 420, "init": 240, "medi": 220, "fina": 380},
        "meem-ar": {"isol": 460, "init": 320, "medi": 300, "fina": 420},
        "heh-ar": {"isol": 400, "init": 360, "medi": 340, "fina": 380},
        "yeh-ar": {"isol": 520, "init": 300, "medi": 240, "fina": 480},
    }
    width = widths[base][form] + 2 * g
    d = Drawing(width)
    left, right = ends(form, width)
    center = (left + right) // 2
    top_y = 320
    bottom_y = -140
    if base in ("beh-ar", "yeh-ar"):
        tick = 220
        if form == "medi":
            teeth = [(center, tick)]
        else:
            teeth = [(right - 40 - t // 2, tick)]
            if form in ("isol", "fina"):
                teeth.append((left + 40 + t // 2, tick))
        d.contours.append(comb(left, right, t, teeth))
        if base == "beh-ar":
            dot(d, center, -150, t)
        else:
            dot(d, center - t, -150, t)
            dot(d, center + t, -150, t)
        # beh and yeh share one bottom anchor, so the contextual kasra rule
        # under yeh is the only difference between them.
        bottom_y = -260
    elif base == "seen-ar":
        step = t + 30
        start = right - 40 - t // 2
        teeth = [(start - index * step, 260) for index in range(3)]
        d.contours.append(comb(left, right, t, teeth))
        center = start - step
        # The rising baseline: each exit sits 80 units above its entry.
        if form in ("init", "medi"):
            d.anchor("exit", 0, 80)
        if form in ("medi", "fina"):
            d.anchor("entry", width, 0)
    elif base == "lam-ar":
        stem = right - 40 - t // 2
        d.contours.append(comb(left, right, t, [(stem, 700)]))
        center = stem
        top_y = 760
    elif base == "meem-ar":
        d.contours.append(comb(left, right, t, []))
        loop = right - 40 - 90 - g
        d.contours += ring(loop, 110 + g // 2, 90 + g, 110 + g // 2, t)
        center = loop
    elif base == "heh-ar":
        d.contours.append(comb(left, right, t, []))
        d.contours += ring(center, 170 + g // 2, 120 + g, 170 + g // 2, t)
        top_y = 400 + g
    d.anchor("top", center, top_y)
    d.anchor("bottom", center, bottom_y)
    return d


def right_letter(base, form, t):
    g = t - 80
    if base == "alef-ar":
        width = 200 + g
        d = Drawing(width)
        x = 60
        if form == "isol":
            d.contours.append(rectangle(x, 0, x + t, 700))
        else:
            d.contours.append([(x, 0), (x, 700), (x + t, 700), (x + t, t),
                               (width, t), (width, 0)])
        d.anchor("top", x + t // 2, 760)
        d.anchor("bottom", x + t // 2, -140)
        return d
    # reh: a slanted stroke that descends below the baseline.
    width = 360 + 2 * g
    d = Drawing(width)
    top = width - 60
    d.contours.append([(top - t, 220), (top, 220), (60 + t, -260), (60, -260)])
    if form == "fina":
        d.contours.append(rectangle(top - t, 0, width, t))
    d.anchor("top", top - t // 2, 320)
    d.anchor("bottom", 140, -340)
    return d


def ligature(form, t):
    """Lam-alef. Component 1 is lam (logical first, drawn on the right)."""
    g = t - 80
    width = 520 + 2 * g
    d = Drawing(width)
    left, right = ends("fina" if form == "fina" else "isol", width)
    alef = left + 40 + t // 2
    lam = right - 40 - t // 2 if form == "isol" else right - 120 - t // 2
    d.contours.append(comb(left, right, t, [(alef, 700), (lam, 700)]))
    d.anchor("top_1", lam, 760)
    d.anchor("top_2", alef, 760)
    d.anchor("bottom_1", lam, -140)
    d.anchor("bottom_2", alef, -140)
    return d


def mark(name, t):
    th = t * 6 // 10
    d = Drawing(0)
    if name == "fatha-ar":
        d.contours.append([(-90, 40), (-90, 40 + th), (90, 100 + th), (90, 100)])
        d.anchor("_top", 0, 0)
    elif name == "kasra-ar":
        d.contours.append([(-90, -100 - th), (-90, -100), (90, -40), (90, -40 - th)])
        d.anchor("_bottom", 0, 0)
    elif name == "damma-ar":
        d.contours += ring(0, 120, 60, 80, th)
        d.anchor("_top", 0, 0)
    elif name == "sukun-ar":
        d.curves.append((0, 110, 70, 70, 1))
        d.curves.append((0, 110, 70 - th, 70 - th, -1))
        d.anchor("_top", 0, 0)
    elif name == "shadda-ar":
        d.contours.append(comb(-120, 120, th, [(-80, 160), (0, 160), (80, 160)]))
        # Offset the comb upward so the mark clears its base.
        d.contours[-1] = [(x, y + 40) for x, y in d.contours[-1]]
        d.anchor("_top", 0, 0)
        d.anchor("top", 0, 240)
    elif name == "hamzaabove-ar":
        d.contours.append([(-60, 40), (-60, 40 + th), (40, 160), (60 + th // 2, 160),
                           (60 + th // 2, 120), (-60 + th, 40)])
        d.anchor("_top", 0, 0)
    elif name == "alefabove-ar":
        d.contours.append(rectangle(-th // 2, 40, th // 2, 260))
        d.anchor("_top", 0, 0)
    return d


def latin(name, t):
    g = t - 80
    width = 0 if name == ".notdef" else 600 + 2 * g
    d = Drawing(width)
    left, right = 60, width - 60
    if name == ".notdef":
        d.width = 500
        d.contours.append(rectangle(60, 0, 440, 700))
        d.contours.append(list(reversed(rectangle(60 + t, t, 440 - t, 700 - t))))
    elif name == "space":
        d.width = 260
    elif name == "period":
        d.width = 120 + t
        d.contours.append(rectangle(60, 0, 60 + t, t))
    elif name == "zero":
        d.curves.append((width // 2, 350, 240 + g, 350, 1))
        d.curves.append((width // 2, 350, 240 - t + g, 350 - t, -1))
    elif name == "one":
        d.contours.append(rectangle(width // 2 - t // 2, 0, width // 2 + t // 2, 700))
    elif name == "two":
        d.contours.append([(left, 0), (left, t), (right - t, 700 - t), (left, 700 - t),
                           (left, 700), (right, 700), (right, 700 - t), (left + t, t),
                           (right, t), (right, 0)])
    elif name == "A":
        mid = width // 2
        d.contours.append([(left, 0), (mid - t // 2, 700), (mid + t // 2, 700), (right, 0),
                           (right - t, 0), (mid, 560), (left + t, 0)])
        d.anchor("top", mid, 760)
    elif name == "V":
        mid = width // 2
        d.contours.append([(left, 700), (left + t, 700), (mid, 140), (right - t, 700),
                           (right, 700), (mid + t // 2, 0), (mid - t // 2, 0)])
    elif name == "T":
        mid = width // 2
        d.contours.append([(mid - t // 2, 0), (mid - t // 2, 700 - t), (left, 700 - t),
                           (left, 700), (right, 700), (right, 700 - t), (mid + t // 2, 700 - t),
                           (mid + t // 2, 0)])
    elif name in ("a", "n", "o"):
        if name == "o":
            d.curves.append((width // 2, 250, 240 + g, 250, 1))
            d.curves.append((width // 2, 250, 240 - t + g, 250 - t, -1))
        else:
            d.contours.append([(left, 0), (left, 500), (right, 500), (right, 0),
                               (right - t, 0), (right - t, 500 - t), (left + t, 500 - t),
                               (left + t, 0)])
            if name == "a":
                d.contours.append(rectangle(left + t, 220, right - t, 220 + t))
    return d


def noon(style, t):
    """Unfinished master: Bold has a fifth point, so the contour cannot interpolate."""
    d = Drawing(460 + 2 * (t - 80))
    right = d.width - 60
    if style == "Regular":
        d.contours.append(rectangle(60, 0, right, t))
    else:
        d.contours.append([(60, 0), (60, t), (right, t), (right, 0), (d.width // 2, -40)])
    dot(d, d.width // 2, 300, t)
    d.anchor("top", d.width // 2, 400)
    d.anchor("bottom", d.width // 2, -140)
    return d


def dal(style, t):
    """Unfinished master: only Bold has the `top` anchor."""
    d = Drawing(340 + 2 * (t - 80))
    right = d.width - 60
    d.contours.append([(60, 0), (60, t), (right - t, t), (right - t, 300),
                       (right, 300), (right, 0)])
    if style == "Bold":
        d.anchor("top", right - t // 2, 400)
    d.anchor("bottom", d.width // 2, -140)
    return d


def draw(name, style, t):
    base, _, form = name.partition(".")
    if name == INCOMPATIBLE[0]:
        return noon(style, t)
    if name == PARTIAL_ANCHOR[0]:
        return dal(style, t)
    if base in dict(DUAL):
        return dual_letter(base, form or "isol", t)
    if base in dict(RIGHT):
        return right_letter(base, form or "isol", t)
    if base == "alefHamzaabove-ar":
        d = right_letter("alef-ar", "isol", t)
        hamza = mark("hamzaabove-ar", t)
        x = d.anchors[0][1]
        d.contours += [[(px + x, py + 760) for px, py in c] for c in hamza.contours]
        d.anchors[0] = ("top", x, 960)
        return d
    if base == "lam_alef-ar":
        return ligature(form or "isol", t)
    if base in dict(MARKS):
        return mark(base, t)
    return latin(name, t)


def write_contour(outline, points):
    element = ET.SubElement(outline, "contour")
    for x, y in points:
        ET.SubElement(element, "point", x=str(x), y=str(y), type="line")


def write_oval(outline, cx, cy, rx, ry, direction):
    # Four cubic segments, with opposite winding for the counter.
    kx, ky = round(rx * 0.55228475), round(ry * 0.55228475)
    points = [(0, -ry, "curve"), (-kx, -ry, None), (-rx, -ky, None),
              (-rx, 0, "curve"), (-rx, ky, None), (-kx, ry, None),
              (0, ry, "curve"), (kx, ry, None), (rx, ky, None),
              (rx, 0, "curve"), (rx, -ky, None), (kx, -ry, None)]
    element = ET.SubElement(outline, "contour")
    for x, y, kind in points:
        point = ET.SubElement(element, "point", x=str(cx + direction * x), y=str(cy + y))
        if kind is not None:
            point.set("type", kind)
            point.set("smooth", "yes")


def category(name):
    if name.partition(".")[0] in dict(MARKS):
        return "mark"
    if name.startswith("lam_alef-ar"):
        return "ligature"
    return "base"


FEATURES = """\
languagesystem DFLT dflt;
languagesystem arab dflt;
languagesystem latn dflt;

@dual = [beh-ar seen-ar lam-ar meem-ar heh-ar yeh-ar];
@dual.init = [beh-ar.init seen-ar.init lam-ar.init meem-ar.init heh-ar.init yeh-ar.init];
@dual.medi = [beh-ar.medi seen-ar.medi lam-ar.medi meem-ar.medi heh-ar.medi yeh-ar.medi];
@joining = [alef-ar reh-ar beh-ar seen-ar lam-ar meem-ar heh-ar yeh-ar];
@joining.fina = [alef-ar.fina reh-ar.fina beh-ar.fina seen-ar.fina lam-ar.fina meem-ar.fina
    heh-ar.fina yeh-ar.fina];

feature ccmp {
    lookup ccmp_alef_hamza {
        sub alefHamzaabove-ar by alef-ar hamzaabove-ar;
    } ccmp_alef_hamza;
} ccmp;

feature init {
    lookup init_forms {
        sub @dual by @dual.init;
    } init_forms;
} init;

feature medi {
    lookup medi_forms {
        sub @dual by @dual.medi;
    } medi_forms;
} medi;

feature fina {
    lookup fina_forms {
        sub @joining by @joining.fina;
    } fina_forms;
} fina;

feature rlig {
    lookup rlig_lam_alef {
        lookupflag IgnoreMarks;
        sub lam-ar.init alef-ar.fina by lam_alef-ar;
        sub lam-ar.medi alef-ar.fina by lam_alef-ar.fina;
    } rlig_lam_alef;
} rlig;

feature curs {
    # Automatic Code
} curs;

feature kern {
    # Automatic Code
    # A fatha on reh needs more room before the next letter.
    lookup kern_reh_fatha {
        pos [reh-ar reh-ar.fina]' <40 0 40 0> fatha-ar;
    } kern_reh_fatha;
    # This pair applies across any marks between reh and heh.
    lookup kern_reh_heh {
        lookupflag IgnoreMarks;
        pos reh-ar heh-ar <-30 0 -30 0>;
    } kern_reh_heh;
} kern;

feature mark {
    # Automatic Code
    # Lower kasra below the two dots of yeh, after the generated attachment.
    lookup kasra_below_yeh {
        pos [yeh-ar yeh-ar.init yeh-ar.medi yeh-ar.fina] kasra-ar' <0 -80 0 0>;
    } kasra_below_yeh;
} mark;

feature mkmk {
    # Automatic Code
} mkmk;
"""


def master(style, t):
    root = ROOT / f"Arabic-{style}.ufo"
    if root.exists():
        shutil.rmtree(root)
    glyphs_dir = root / "glyphs"
    glyphs_dir.mkdir(parents=True)
    glyphs = glyph_list()
    plist(root / "metainfo.plist", {"creator": "org.runebender.tests", "formatVersion": 3})
    plist(root / "layercontents.plist", [["public.default", "glyphs"]])
    plist(root / "fontinfo.plist", {
        "familyName": FAMILY, "styleName": style,
        "unitsPerEm": 1000, "ascender": 900, "descender": -400,
        "capHeight": 700, "xHeight": 500,
        "openTypeHeadCreated": "2026/01/01 00:00:00",
        "openTypeOS2WeightClass": 400 if style == "Regular" else 700,
        "versionMajor": 1, "versionMinor": 0,
    })
    plist(root / "lib.plist", {
        "public.glyphOrder": [name for name, _ in glyphs],
        "public.openTypeCategories": {name: category(name) for name, _ in glyphs},
    })
    plist(root / "groups.plist", {
        "public.kern1.reh": ["reh-ar", "reh-ar.fina"],
        "public.kern2.alef": ["alef-ar", "alefHamzaabove-ar"],
        "public.kern1.A": ["A"],
        "public.kern2.V": ["V"],
    })
    plist(root / "kerning.plist", {
        "public.kern1.reh": {"public.kern2.alef": -60},
        "reh-ar": {"meem-ar": -40},
        "public.kern1.A": {"public.kern2.V": -70},
    })
    (root / "features.fea").write_text(FEATURES, encoding="utf-8")
    contents = {}
    for index, (name, codepoint) in enumerate(glyphs):
        drawing = draw(name, style, t)
        glyph = ET.Element("glyph", name=name, format="2")
        ET.SubElement(glyph, "advance", width=str(drawing.width))
        if codepoint is not None:
            ET.SubElement(glyph, "unicode", hex=f"{codepoint:04X}")
        for anchor, x, y in drawing.anchors:
            ET.SubElement(glyph, "anchor", name=anchor, x=str(x), y=str(y))
        outline = ET.SubElement(glyph, "outline")
        for points in drawing.contours:
            write_contour(outline, points)
        for cx, cy, rx, ry, direction in drawing.curves:
            write_oval(outline, cx, cy, rx, ry, direction)
        filename = f"glyph{index:02}.glif"
        contents[name] = filename
        ET.indent(glyph, space="  ")
        ET.ElementTree(glyph).write(glyphs_dir / filename, encoding="UTF-8", xml_declaration=True)
    plist(glyphs_dir / "contents.plist", contents)


DESIGNSPACE = """\
<?xml version="1.0" encoding="UTF-8"?>
<designspace format="5.0">
  <axes>
    <axis name="Weight" tag="wght" minimum="400" default="400" maximum="700"/>
  </axes>
  <sources>
    <source filename="Arabic-Regular.ufo" familyname="{family}" stylename="Regular">
      <location><dimension name="Weight" xvalue="400"/></location>
    </source>
    <source filename="Arabic-Bold.ufo" familyname="{family}" stylename="Bold">
      <location><dimension name="Weight" xvalue="700"/></location>
    </source>
  </sources>
  <instances>
    <instance name="Regular" stylename="Regular"><location><dimension name="Weight" xvalue="400"/></location></instance>
    <instance name="Bold" stylename="Bold"><location><dimension name="Weight" xvalue="700"/></location></instance>
  </instances>
</designspace>
"""


if __name__ == "__main__":
    master("Regular", 80)
    master("Bold", 120)
    (ROOT / "Arabic.designspace").write_text(DESIGNSPACE.format(family=FAMILY), encoding="utf-8")
