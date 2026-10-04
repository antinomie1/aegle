# Aegle

A modular retained-mode GUI library in Rust 2024, focused on low memory use,
event-driven updates, CJK and native input methods. GPU and software rendering
are planned as independently selectable backends.

Implementation is in progress: compact shared types, retained trees, Taffy
layout, retained Unicode paragraphs, on-demand CJK glyphs, drawing records and
software rasterization work today. Native windows, editing/IME, system
accessibility and GPU backends are not implemented yet.
See the [implementation status](docs/implementation.md) and [design](docs/README.md).

```sh
cargo test --workspace --all-features
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo run -p aegle-render-software --features text --example text_scene --release
cargo doc --workspace --all-features --no-deps
```

Independent crates: `aegle-types` (no_std geometry/colors), `aegle-core` (retained
state with no third-party dependencies), `aegle-layout` (Taffy over that tree),
`aegle-scene` (validated drawing records), `aegle-text` (Parley paragraphs and
font fallback), `aegle-glyph` (on-demand rasterization and a bounded image cache),
and `aegle-render-software` (borrowed framebuffers and linear-light compositing).
Text support is opt-in for scene/software rendering. Geometry-only builds have
no font stack; application font bytes remain shared, with no bundled font atlas.
The examples are headless demonstrations, not working GUI applications.
They write `target/aegle-software.png` and `target/aegle-text.png`. The PNG encoder
is a development dependency; the optional glyph module also uses a PNG decoder
for embedded color font bitmaps. Portable test fonts and their OFL notices are in
`tests/assets/`; library builds embed no fonts.

Licensed under [LGPL-3.0-only](LICENSE); the incorporated GPLv3 text is in [COPYING](COPYING).
