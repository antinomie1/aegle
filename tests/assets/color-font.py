"""Build a tiny COLRv0/sbix fixture from the OFL CJK subset (FontTools 4.62.1)."""

import struct
import zlib
from pathlib import Path

from fontTools import subset
from fontTools.colorLib.builder import buildCOLR, buildCPAL
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.sbixGlyph import Glyph
from fontTools.ttLib.tables.sbixStrike import Strike

root = Path(__file__).parent
font = TTFont(root / "aegle-test-cjk.otf", recalcTimestamp=False)
options = subset.Options()
options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14, 16, 17]
options.name_legacy = True
subsetter = subset.Subsetter(options=options)
subsetter.populate(text="AB")
subsetter.subset(font)
a, b = (font.getBestCmap()[ord(c)] for c in "AB")
font["COLR"] = buildCOLR({a: [(b, 0), (b, 1)]}, version=0)
font["CPAL"] = buildCPAL([[(1, 0, 0, 1), (0, 0, 1, 0.5)]])


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


pixels = bytes([255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 0])
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"\0" + pixels[:8] + b"\0" + pixels[8:])) + chunk(b"IEND", b"")
strike = Strike(ppem=16, resolution=72)
strike.glyphs = {b: Glyph(glyphName=b, graphicType="png ", imageData=png)}
font["sbix"] = newTable("sbix")
font["sbix"].strikes = {16: strike}
advance, _ = font["hmtx"].metrics[b]
font["hmtx"].metrics[b] = (advance, 0)
for record in font["name"].names:
    text = record.toUnicode().replace("Aegle Test CJK", "Aegle Test Color").replace("AegleTestCJK", "AegleTestColor")
    record.string = text.encode(record.getEncoding())
cff = font["CFF "].cff
cff.fontNames = ["AegleTestColor-Regular"]
top = cff.topDictIndex[0]
top.FamilyName = "Aegle Test Color"
top.FullName = "Aegle Test Color Regular"
for index, dictionary in enumerate(top.FDArray):
    dictionary.FontName = f"AegleTestColor-Regular-{index}"
destination = root / "aegle-test-color.otf"
font.save(destination)
print(f"{destination}: {destination.stat().st_size} bytes")
