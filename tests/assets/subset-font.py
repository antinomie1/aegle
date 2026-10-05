"""Rebuild the OFL test font with fonttools 4.62.1 (not a runtime dependency)."""

import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

source = Path(sys.argv[1])
destination = Path(__file__).with_name("aegle-test-cjk.otf")
font = TTFont(source, fontNumber=0, recalcTimestamp=False)
options = subset.Options()
options.hinting = False
options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14, 16, 17]
options.name_legacy = True
options.name_languages = [0x409]
subsetter = subset.Subsetter(options=options)
subsetter.populate(text="".join(map(chr, range(32, 127))) + "é\u0301\u2022你好世界中文日本語한글")
subsetter.subset(font)

names = {
    1: "Aegle Test CJK",
    2: "Regular",
    3: "Aegle Test CJK Regular 1.0",
    4: "Aegle Test CJK Regular",
    5: "Version 1.000; Aegle test subset",
    6: "AegleTestCJK-Regular",
    16: "Aegle Test CJK",
    17: "Regular",
}
for record in font["name"].names:
    if record.nameID in names:
        record.string = names[record.nameID].encode(record.getEncoding())
cff = font["CFF "].cff
cff.fontNames = ["AegleTestCJK-Regular"]
top = cff.topDictIndex[0]
top.FamilyName = "Aegle Test CJK"
top.FullName = "Aegle Test CJK Regular"
top.version = "1.000"
for index, dictionary in enumerate(top.FDArray):
    dictionary.FontName = f"AegleTestCJK-Regular-{index}"
font.save(destination)
print(f"{destination}: {destination.stat().st_size} bytes")
