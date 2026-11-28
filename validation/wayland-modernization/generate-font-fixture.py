"""Generate an original tiny OpenType shaping fixture.

Usage: uv run --no-project --with fonttools==4.61.1 python generate-font-fixture.py OUTPUT
"""
from sys import argv
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

order = [".notdef", "space", "A", "V", "f", "i", "fi", "j"]
advances = [600, 250, 600, 600, 400, 350, 750, 450]
bounds = {"A": (0, 0, 500, 700), "V": (25, 0, 550, 650),
          "f": (10, 0, 360, 700), "i": (20, 0, 300, 500),
          "fi": (10, 0, 680, 700), "j": (-100, -200, 400, 700)}
fb = FontBuilder(1000, isTTF=True)
fb.setupGlyphOrder(order)
fb.setupCharacterMap({32: "space", **{ord(ch): ch for ch in "AVfij"}})
glyphs = {}
metrics = {}
for name, advance in zip(order, advances):
    pen = TTGlyphPen(None)
    if name in bounds:
        x0, y0, x1, y1 = bounds[name]
        pen.moveTo((x0, y0))
        pen.lineTo((x1, y0))
        pen.lineTo((x1, y1))
        pen.lineTo((x0, y1))
        pen.closePath()
    glyphs[name] = pen.glyph()
    metrics[name] = (advance, bounds.get(name, (0, 0, 0, 0))[0])
fb.setupGlyf(glyphs)
fb.setupHorizontalMetrics(metrics)
fb.setupHorizontalHeader(ascent=800, descent=-200)
fb.setupNameTable({"familyName": "Original Shaping Fixture", "styleName": "Regular",
    "uniqueFontIdentifier": "OriginalShapingFixture-Regular",
    "fullName": "Original Shaping Fixture Regular", "psName": "OriginalShapingFixture-Regular"})
fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
fb.setupPost()
addOpenTypeFeaturesFromString(fb.font, """
languagesystem DFLT dflt;
languagesystem latn dflt;
feature kern { pos A V -120; } kern;
feature liga { sub f i by fi; } liga;
""")
fb.font["head"].created = fb.font["head"].modified = 2082844800
fb.font.recalcTimestamp = False
fb.save(argv[1])
