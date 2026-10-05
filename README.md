# Aegle

A modular retained-mode GUI library in Rust 2024, focused on low memory use,
event-driven updates, CJK and native input methods. Software rendering is
available; independently selectable GPU backends are planned.

Implementation is in progress: compact shared types, retained trees, Taffy
layout, retained Unicode paragraphs/editors, on-demand CJK glyphs, drawing records
and software rasterization work today. The independent Wayland backend adds
native windows, bounded SHM presentation, keyboard/pointer input and text-input-v3.
The application layer offers imperative windows, rows, columns, labels, buttons
and plain text fields with light/dark/high-contrast themes. Shared control
behavior connects input and focus to the retained tree and editor.
Optional Unix accessibility exposes controls, CJK
text, selection, focus and button actions through AT-SPI. Native text replacement,
other OS backends and GPU rendering remain unimplemented.
See the [implementation status](docs/implementation.md) and [design](docs/README.md).

```rust
use aegle::prelude::*;
fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界")?;
    app.run()
}
```

The current `aegle` defaults are Linux Wayland, software rendering, system fonts
and Unix accessibility. System fonts must cover the requested text. Native IME
requires text-input-v3; focusing an editable field without it returns a capability
error. GPU, Windows/macOS hosts, animation and markup remain in development.

```sh
cargo run -p aegle --example hello --release
cargo run -p aegle --example controls --release
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
`aegle-controls` (unskinned Button and optional TextField behavior),
`aegle-access` (UI-thread callback mailbox and optional Unix accessibility),
`aegle-platform-wayland` (windows and native input, independent of rendering),
`aegle-theme` (allocation-free typed palettes/metrics), and `aegle-app` (retained
imperative UI, with the native host behind features).

For a system-font software application without the Unix accessibility adapter:

```sh
cargo run -p aegle --no-default-features --features wayland,system-fonts --example controls --release
```

For an existing rendering/window host, use `aegle-app` without default features
and construct `Ui::with_fonts` with an explicit shared font collection. The same
controls expose scene visits, normalized input, bounded IME state and optional
semantic updates. Control handles are weak: dropping a handle keeps its control,
while removing a subtree or closing its window invalidates its handles.

Text support is opt-in for scene/software rendering. Geometry-only builds have
no font stack; application font bytes remain shared, with no bundled font atlas.
The renderer examples are headless demonstrations.
They write `target/aegle-software.png`, `target/aegle-text.png` and
`target/aegle-editor.png`. The editor example verifies preedit, commit and undo
while recording selection/caret decorations into the same scene. The PNG encoder
is a development dependency; the optional glyph module also uses a PNG decoder
for embedded color font bitmaps. Portable test fonts and their OFL notices are in
`tests/assets/`; library builds embed no fonts.

The Wayland `editor` example combines a CJK text field and button in one retained
Taffy tree, with routed actions, Tab focus and pointer capture. Keyboard editing
and IME use the shared controls and atomic text transaction APIs.
It needs a Wayland compositor with xdg-shell and wl_compositor version 4 or newer;
native composition additionally needs text-input-v3 and an input method. Building
requires libxkbcommon development metadata for pkg-config; running needs the
libxkbcommon runtime. The independent platform example remains a low-level
composition; `aegle --example controls` demonstrates the application API. System
font discovery on Linux also links system Fontconfig and its distribution-specific
runtime dependencies. Explicit application fonts can avoid this dependency.

Enable the native accessibility example with:

```sh
cargo run -p aegle-platform-wayland --example editor --features example-accessibility --release
```

This needs session D-Bus and AT-SPI services. The adapter shares the existing
control/editor state and wakes the UI without polling. It adds a process-wide
worker and semantic cache; current limitations, including missing EditableText
and Wayland screen positioning, are documented in [accessibility](docs/accessibility.md).

Licensed under [LGPL-3.0-only](LICENSE); the incorporated GPLv3 text is in [COPYING](COPYING).
