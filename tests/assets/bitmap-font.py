"""Rebuild the bitmap-only test font with fonttools 4.62.1 (not a runtime dependency)."""

import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

source = Path(sys.argv[1])
destination = Path(__file__).with_name("aegle-test-bitmap.ttf")
font = TTFont(source, recalcTimestamp=False)
options = subset.Options()
options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14]
options.name_legacy = True
options.name_languages = [0x409]
subsetter = subset.Subsetter(options=options)
subsetter.populate(text="\U0001f3af")
subsetter.subset(font)
names = {
    1: "Aegle Test Bitmap",
    2: "Regular",
    3: "Aegle Test Bitmap Regular 1.0",
    4: "Aegle Test Bitmap Regular",
    5: "Version 1.000; Aegle test subset",
    6: "AegleTestBitmap-Regular",
}
for record in font["name"].names:
    if record.nameID in names:
        record.string = names[record.nameID].encode(record.getEncoding())
font.save(destination)
print(f"{destination}: {destination.stat().st_size} bytes")
