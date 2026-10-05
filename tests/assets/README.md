# Text fixture

`aegle-test-cjk.otf` is a small, modified subset of **Noto Sans CJK JP Regular
2.004**, licensed under the SIL Open Font License 1.1 in [OFL.txt](OFL.txt).
The original font's copyright and license notices are also retained inside the
font. This fixture is for portable shaping and rasterization tests; it is not a
bundled application font or a substitute for a complete CJK font.

The subset contains printable ASCII (`U+0020–U+007E`), `é`, combining acute
(`U+0301`), the password mask `•` (`U+2022`), and `你好世界中文日本語한글`, with
relevant OpenType layout glyphs.
Hinting is removed. SFNT family, full, unique and PostScript names and CFF names
are changed to **Aegle Test CJK**; the outlines are unchanged. Japanese-source
ideograph forms are intentional.

Source: [notofonts/noto-cjk](https://github.com/notofonts/noto-cjk), Debian
`fonts-noto-cjk` version `1:20240730+repack1-1`, face 0 of the package's
`opentype/noto/NotoSansCJK-Regular.ttc` font file. Its SHA-256 is
`b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a`.
The standalone license text comes from the package's `copyright` document; the
Adobe notice comes from the font's name table.

To reproduce from the same source, with Python and FontTools 4.62.1:

```sh
python3 -m pip install --target target/fonttools fonttools==4.62.1
PYTHONPATH=target/fonttools python3 tests/assets/subset-font.py \
  path/to/NotoSansCJK-Regular.ttc
```

The script and FontTools are regeneration tools, not release dependencies.
Ordinary builds and tests use the checked-in 18,240-byte OTF directly.

`aegle-test-color.otf` is a further 2,924-byte OFL derivative, renamed **Aegle
Test Color**. It retains only A/B outlines and the original notices. Its A glyph
uses two identical B outlines in COLRv0: opaque red under half-transparent blue.
Its B glyph has a 2×2 sbix PNG strike at 16 ppem, containing half-transparent red,
opaque green, opaque blue and transparent white. The B left bearing is set to
zero for exact pixel placement. These synthetic colors verify gamma, alpha,
bitmap decoding and per-glyph source selection; they are not application assets.

Regenerate after the CJK fixture with:

```sh
PYTHONPATH=target/fonttools python3 tests/assets/color-font.py
```
