# Aegle

A modular retained-mode GUI library in Rust 2024, focused on low memory use,
event-driven updates, CJK and native input methods. GPU and software rendering
are planned as independently selectable backends.

Implementation is in progress: compact shared types, retained trees, Taffy
layout, retained Unicode paragraphs/editors, on-demand CJK glyphs, drawing records
and software rasterization work today. The independent Wayland backend adds
native windows, bounded SHM presentation, keyboard/pointer input and text-input-v3.
System accessibility, other OS backends and GPU rendering remain unimplemented.
See the [implementation status](docs/implementation.md) and [design](docs/README.md).

```sh
cargo test --workspace --all-features
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo run -p aegle-render-software --features text --example text_scene --release
cargo run -p aegle-render-software --features text --example editor_scene --release
cargo run -p aegle-platform-wayland --example editor --release
cargo doc --workspace --all-features --no-deps
```

Independent crates: `aegle-types` (no_std geometry/colors), `aegle-core` (retained
state with no third-party dependencies), `aegle-layout` (Taffy over that tree),
`aegle-scene` (validated drawing records), `aegle-text` (paragraphs, font fallback,
plain editing, composition and bounded delta undo), `aegle-glyph` (on-demand
rasterization and a bounded image cache),
`aegle-render-software` (borrowed framebuffers and linear-light compositing),
and `aegle-platform-wayland` (windows and native input, independent of rendering).
Text support is opt-in for scene/software rendering. Geometry-only builds have
no font stack; application font bytes remain shared, with no bundled font atlas.
The renderer examples are headless demonstrations.
They write `target/aegle-software.png`, `target/aegle-text.png` and
`target/aegle-editor.png`. The editor example verifies preedit, commit and undo
while recording selection/caret decorations into the same scene. The PNG encoder
is a development dependency; the optional glyph module also uses a PNG decoder
for embedded color font bitmaps. Portable test fonts and their OFL notices are in
`tests/assets/`; library builds embed no fonts.

The Wayland `editor` example is an interactive software-rendered CJK text field.
It needs a Wayland compositor with xdg-shell and wl_compositor version 4 or newer;
native composition additionally needs text-input-v3 and an input method. Building
requires libxkbcommon development metadata for pkg-config; running needs the
libxkbcommon runtime. This low-level example is not yet the planned widget/app API.

Licensed under [LGPL-3.0-only](LICENSE); the incorporated GPLv3 text is in [COPYING](COPYING).
