# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Regenerate the original synthetic Core font using only Python's standard library.

No outlines or metadata are copied from an external font. These deliberately
simple polygons exercise loading, interpolation, metrics, kerning, components,
anchors and compilation; they do not stand in for Virtua's script coverage.
Run: python3 tests/fixtures/core/generate.py
"""

from pathlib import Path
import plistlib
from xml.etree import ElementTree as ET

ROOT = Path(__file__).resolve().parent


def plist(path, value):
    path.write_bytes(plistlib.dumps(value, sort_keys=True))


def contour(outline, points):
    element = ET.SubElement(outline, "contour")
    for x, y in points:
        ET.SubElement(element, "point", x=str(x), y=str(y), type="line")


def rectangle(x0, y0, x1, y1):
    return [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]


def oval(outline, cx, cy, rx, ry, direction=1):
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


def master(style, growth):
    root = ROOT / f"Core-{style}.ufo"
    glyphs = root / "glyphs"
    glyphs.mkdir(parents=True, exist_ok=True)
    plist(root / "metainfo.plist", {"creator": "org.runebender.tests", "formatVersion": 3})
    plist(root / "layercontents.plist", [["public.default", "glyphs"]])
    plist(root / "fontinfo.plist", {
        "familyName": "Runebender Core Fixture", "styleName": style,
        "unitsPerEm": 1024, "ascender": 800, "descender": -224,
        "capHeight": 700, "xHeight": 500,
        "openTypeHeadCreated": "2026/01/01 00:00:00",
        "openTypeOS2WeightClass": 400 if style == "Regular" else 700,
        "versionMajor": 1, "versionMinor": 0,
    })
    names = {char: ord(char) for char in "ABHORTVabdefghinorsu"}
    names.update({".notdef": None, "space": 32, "period": 46, "zero": 48,
                  "one": 49, "gravecomb": 0x300, "ringcomb": 0x30A, "Agrave": 0xC0})
    plist(root / "lib.plist", {"public.glyphOrder": list(names)})
    plist(root / "groups.plist", {"public.kern1.A": ["A"], "public.kern2.V": ["V"]})
    plist(root / "kerning.plist", {"public.kern1.A": {"public.kern2.V": -64}, "one": {"a": -16}})
    (root / "features.fea").write_text("languagesystem DFLT dflt;\nlanguagesystem latn dflt;\n", encoding="utf-8")
    contents = {}
    for index, (name, codepoint) in enumerate(names.items()):
        glyph = ET.Element("glyph", name=name, format="2")
        mark = name.endswith("comb")
        width = 0 if mark else (320 if name == "space" else 640 + growth)
        ET.SubElement(glyph, "advance", width=str(width))
        if codepoint is not None:
            ET.SubElement(glyph, "unicode", hex=f"{codepoint:04X}")
        if name in ("A", "n"):
            ET.SubElement(glyph, "anchor", name="top", x="320", y="720" if name == "A" else "520")
        if mark:
            ET.SubElement(glyph, "anchor", name="_top", x="0", y="0")
        outline = ET.SubElement(glyph, "outline")
        left, right, stem = 64 - growth, 576 + growth, 64 + growth * 2
        if name == "Agrave":
            ET.SubElement(outline, "component", base="A")
            ET.SubElement(outline, "component", base="gravecomb", xOffset="320", yOffset="720")
        elif name == "H":
            contour(outline, [(left, 0), (left, 700), (left + stem, 700),
                             (left + stem, 400), (right - stem, 400), (right - stem, 700),
                             (right, 700), (right, 0), (right - stem, 0),
                             (right - stem, 300), (left + stem, 300), (left + stem, 0)])
        elif name in ("O", "zero"):
            oval(outline, 320, 350, 256 + growth, 350)
            oval(outline, 320, 350, 192 - growth, 270, direction=-1)
        elif name in ("n", "h"):
            contour(outline, [(left, 0), (left, 500), (right, 500), (right, 0),
                             (right - stem, 0), (right - stem, 420),
                             (left + stem, 420), (left + stem, 0)])
        elif mark:
            contour(outline, rectangle(-32, 16, 32, 80))
        elif name == "period":
            contour(outline, rectangle(64, 0, 128, 64))
        elif name != "space":
            contour(outline, rectangle(left, 0, right, 700 if name.isupper() else 500))
        filename = f"glyph{index:02}.glif"
        contents[name] = filename
        ET.indent(glyph, space="  ")
        ET.ElementTree(glyph).write(glyphs / filename, encoding="UTF-8", xml_declaration=True)
    plist(glyphs / "contents.plist", contents)


if __name__ == "__main__":
    master("Regular", 0)
    master("Bold", 32)
