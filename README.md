# Aegle

A modular retained-mode GUI library in Rust 2024, focused on low memory use,
event-driven updates, CJK and native input methods. Software rendering and an
Vulkan geometry/text rendering are available, including direct native window presentation.

Implementation is in progress: compact shared types, retained trees, Taffy
layout, retained Unicode paragraphs/editors, on-demand CJK glyphs, drawing records
and software rasterization work today. The independent Wayland backend adds
native windows, bounded SHM presentation, keyboard/pointer input and text-input-v3.
The application layer offers windows, rows, columns, scroll views, labels, buttons and plain
text fields, checkboxes, switches, sliders and progress bars with light/dark/high-contrast themes. Compiled `.aegle` markup and
simple imperative Rust create the same retained controls. Local colors, typography
and small theme/state skin functions let component libraries reuse those controls. Shared control
behavior connects input and focus to the retained tree and editor.
Optional paint transitions share the same state, with frame-driven sampling,
smooth retargeting and explicit reduced-motion support.
Optional Unix accessibility exposes controls, CJK
text, selection, focus and button actions through AT-SPI; AT-SPI text replacement remains unsupported.
Windows adds Win32 windows, software/Vulkan presentation, IMM composition and optional UIA. TSF, macOS and full assistive-technology acceptance remain unfinished.
See the [implementation status](docs/implementation.md) and [design](docs/README.md).

Write `main.aegle` next to your package's Cargo.toml:

```text
Window {
    title: "Hello"
    Text { text: "你好，世界" }
}
```

```rust
fn main() -> aegle::Result<()> {
    aegle::App::run_ui(aegle::ui!("main.aegle"))
}
```

Markup compiles to direct constructors and setters. Named `id` fields return
typed weak handles for ordinary Rust callbacks; the executable carries no markup
parser or runtime registry. Static literals are supported; bindings, event blocks,
component imports and runtime loading remain in development.

The imperative equivalent is also small:

```rust
use aegle::prelude::*;
fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界")?;
    app.run()
}
```

Developer documentation (Chinese): the [API guide](docs/developer/api.md) and the
[control reference](docs/developer/controls.md) with screenshots of every default control in its states.
Regenerate the screenshots with `cargo run -p aegle --example gallery`.

The current `aegle` defaults are native windows (Linux Wayland / Windows Win32), software rendering and system fonts,
with markup (including the dynamic markup engine) and transitions enabled. System accessibility adapters are opt-in:
`--features unix-accessibility` (AT-SPI; needs session D-Bus and adds the zbus stack) or
`--features windows-accessibility` (UI Automation). System fonts must cover the requested text. Wayland IME
requires text-input-v3; focusing an editable field without it returns a capability
error. Windows currently uses IMM compatibility, not a TSF text store. macOS remains in development.

```sh
cargo run -p aegle --example hello --release
cargo run -p aegle --example controls --release
cargo run -p aegle --example hello_markup --release
cargo run -p aegle --example markup_controls --release
cargo run -p aegle --example components --release
cargo run -p aegle --example widgets --release
cargo run -p aegle --example scrolling --release
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
`aegle-render-vulkan` (geometry/text, native swapchains, bounded allocations and explicit offscreen readback),
`aegle-controls` (unskinned Button/Toggle/Slider behavior, shared numeric Range and optional TextField),
`aegle-access` (UI-thread callback mailbox and optional Unix/Windows accessibility),
`aegle-platform-wayland` and `aegle-platform-win32` (windows and native input, independent of rendering),
`aegle-theme` (allocation-free palettes, state-based skins and local style values),
`aegle-motion` (allocation-free elapsed-time tweens), `aegle-app` (retained
imperative UI, with the native host behind features), and `aegle-markup` (bounded
parsing and static component checking). `aegle-macros` generates compiled views.

For a system-font software application without markup or transitions:

```sh
cargo run -p aegle --no-default-features --features native,software,system-fonts --example controls --release
```

For a Vulkan-only application, with no software renderer in its runtime dependencies:

```sh
cargo run -p aegle --no-default-features --features native,vulkan,system-fonts,markup --example scrolling --release
```

When both renderers are compiled, set `AppOptions.renderer` to `RendererBackend::Vulkan` explicitly; software remains the default. Driver/feature/budget failures return errors. A minimal Vulkan-only build defaults to Vulkan. Native Vulkan currently creates a device per window; large windows may require increasing `AppOptions.vulkan.memory_budget`.

For an existing rendering/window host, use `aegle-app` without default features
and construct `Ui::with_fonts` with an explicit shared font collection. The same
controls expose scene visits, normalized input, bounded IME state and optional
semantic updates. Control handles are weak: dropping a handle keeps its control,
while removing a subtree or closing its window invalidates its handles.

Text support is opt-in for scene, software and Vulkan rendering. Geometry-only builds have
no font stack; application font bytes remain shared, with no bundled font atlas.
The software renderer examples are headless demonstrations.
They write `target/aegle-software.png`, `target/aegle-text.png` and
`target/aegle-editor.png`. The editor example verifies preedit, commit and undo
while recording selection/caret decorations into the same scene. The PNG encoder
is a development dependency; the optional glyph module also uses a PNG decoder
for embedded color font bitmaps. Portable test fonts and their OFL notices are in
`tests/assets/`; library builds embed no fonts.

The independent Vulkan examples require a Vulkan 1.1 loader and a compatible
driver. Optional `text` uses on-demand R8/RGBA8_SRGB atlas pages and shared glyph
rasterization; the renderer does not depend on Parley. The examples write PPM
using only the standard library. Drawing blends in
an RGBA16F linear attachment, then a GPU pass encodes premultiplied sRGB RGBA8.
Readback is explicit. See the [Vulkan contract](docs/vulkan.md) for limits and
device verification; a CPU Vulkan driver is not hardware acceleration.

```sh
cargo run -p aegle-render-vulkan --example geometry --release -- /tmp/aegle-vulkan.ppm
cargo run -p aegle-render-vulkan --features text --example text_scene --release -- /tmp/aegle-vulkan-text.ppm
```

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
