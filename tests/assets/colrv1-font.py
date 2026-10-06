"""Build tiny COLRv1 and OpenType-SVG fixtures from scratch (FontTools 4.62.1).

One square glyph "box" (0..1000 em units) and the colored glyph "A", whose
paint graph exercises a gradient fill, a transform, a foreground-colored layer,
a radial gradient, a multiply composite and a sweep gradient. Run with
`uv run --with fonttools python colrv1-font.py`.
"""

from pathlib import Path

from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables import otTables as ot

pen = TTGlyphPen(None)
pen.moveTo((0, 0))
pen.lineTo((0, 1000))
pen.lineTo((1000, 1000))
pen.lineTo((1000, 0))
pen.closePath()
box = pen.glyph()
empty = TTGlyphPen(None).glyph()

builder = FontBuilder(1000, isTTF=True)
builder.setupGlyphOrder([".notdef", "A", "box"])
builder.setupCharacterMap({0x41: "A"})
builder.setupGlyf({".notdef": empty, "A": empty, "box": box})
builder.setupHorizontalMetrics({".notdef": (1000, 0), "A": (1000, 0), "box": (1000, 0)})
builder.setupHorizontalHeader(ascent=800, descent=-200)
builder.setupNameTable({"familyName": "Aegle Test COLRv1", "styleName": "Regular"})
builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
builder.setupPost()


def line(*stops):
    return {
        "ColorStop": [
            {"StopOffset": at, "PaletteIndex": index, "Alpha": 1.0} for at, index in stops
        ]
    }


# Palette: 0 red, 1 blue, 2 green, 3 yellow.
builder.font["CPAL"] = buildCPAL([[(1, 0, 0, 1), (0, 0, 1, 1), (0, 1, 0, 1), (1, 1, 0, 1)]])
fill = ot.PaintFormat
paint = {
    "Format": fill.PaintColrLayers,
    "Layers": [
        # Whole em square: red at the left edge to blue at the right edge.
        {
            "Format": fill.PaintGlyph,
            "Glyph": "box",
            "Paint": {
                "Format": fill.PaintLinearGradient,
                "ColorLine": line((0, 0), (1, 1)),
                "x0": 0, "y0": 0, "x1": 1000, "y1": 0, "x2": 0, "y2": 1000,
            },
        },
        # A quarter-size box translated to the upper right, in the foreground color.
        {
            "Format": fill.PaintTranslate,
            "dx": 500,
            "dy": 500,
            "Paint": {
                "Format": fill.PaintScale,
                "scaleX": 0.5,
                "scaleY": 0.5,
                "Paint": {
                    "Format": fill.PaintGlyph,
                    "Glyph": "box",
                    "Paint": {"Format": fill.PaintSolid, "PaletteIndex": 0xFFFF, "Alpha": 1.0},
                },
            },
        },
        # Lower-right quarter: a green-to-blue radial gradient multiplied by yellow,
        # so the center stays green and the blue rim turns black.
        {
            "Format": fill.PaintTranslate,
            "dx": 500,
            "dy": 0,
            "Paint": {
                "Format": fill.PaintScale,
                "scaleX": 0.5,
                "scaleY": 0.5,
                "Paint": {
                    "Format": fill.PaintComposite,
                    "CompositeMode": "MULTIPLY",
                    "SourcePaint": {
                        "Format": fill.PaintGlyph,
                        "Glyph": "box",
                        "Paint": {
                            "Format": fill.PaintRadialGradient,
                            "ColorLine": line((0, 2), (1, 1)),
                            "x0": 500, "y0": 500, "r0": 0, "x1": 500, "y1": 500, "r1": 500,
                        },
                    },
                    "BackdropPaint": {
                        "Format": fill.PaintGlyph,
                        "Glyph": "box",
                        "Paint": {"Format": fill.PaintSolid, "PaletteIndex": 3, "Alpha": 1.0},
                    },
                },
            },
        },
        # Upper-left quarter: a full-turn sweep from red (right of center) to blue.
        {
            "Format": fill.PaintTranslate,
            "dx": 0,
            "dy": 500,
            "Paint": {
                "Format": fill.PaintScale,
                "scaleX": 0.5,
                "scaleY": 0.5,
                "Paint": {
                    "Format": fill.PaintGlyph,
                    "Glyph": "box",
                    "Paint": {
                        "Format": fill.PaintSweepGradient,
                        "ColorLine": line((0, 0), (1, 1)),
                        "centerX": 500, "centerY": 500, "startAngle": 0, "endAngle": 360,
                    },
                },
            },
        },
    ],
}
builder.font["COLR"] = buildCOLR({"A": paint}, clipBoxes={"A": (0, 0, 1000, 1000)})
destination = Path(__file__).parent / "aegle-test-colrv1.ttf"
builder.save(destination)
print(f"{destination}: {destination.stat().st_size} bytes")


# OpenType-SVG: glyph 1 ("A") is a red bar with a half-transparent blue square,
# in the document's y-down units relative to the baseline.
from fontTools.ttLib import newTable  # noqa: E402

svg_builder = FontBuilder(1000, isTTF=True)
svg_builder.setupGlyphOrder([".notdef", "A"])
svg_builder.setupCharacterMap({0x41: "A"})
svg_builder.setupGlyf({".notdef": empty, "A": empty})
svg_builder.setupHorizontalMetrics({".notdef": (1000, 0), "A": (1000, 0)})
svg_builder.setupHorizontalHeader(ascent=800, descent=-200)
svg_builder.setupNameTable({"familyName": "Aegle Test SVG", "styleName": "Regular"})
svg_builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
svg_builder.setupPost()
svg_builder.font["SVG "] = newTable("SVG ")
svg_builder.font["SVG "].docList = [
    (
        '<svg xmlns="http://www.w3.org/2000/svg"><g id="glyph1">'
        '<rect x="0" y="-800" width="1000" height="800" fill="#ff0000"/>'
        '<rect x="500" y="-800" width="500" height="400" fill="#0000ff" fill-opacity="0.5"/>'
        "</g></svg>",
        1,
        1,
    )
]
destination = Path(__file__).parent / "aegle-test-svg.ttf"
svg_builder.save(destination)
print(f"{destination}: {destination.stat().st_size} bytes")
