# Aegle

**English** · [简体中文](https://github.com/antinomie1/aegle/blob/main/README.zh-CN.md)

Aegle is a modular retained-mode GUI library for Rust 2024, built for low memory use, event-driven updates, CJK text and native input methods. Controls are created once and changed through handles; nothing redraws or polls while the UI is idle.

It is a workspace of independent crates (retained tree, Taffy layout, text and glyphs, drawing scenes, software (tiny-skia or system cairo) / Vulkan / wgpu renderers, SDL3, Wayland and Win32 platform backends, controls, themes, markup) plus the `aegle` facade that ties them together. Version 0.1 is a preview: the [implementation status](https://github.com/antinomie1/aegle/blob/main/docs/implementation.md) records what works and what has been verified on each platform, and its known limitations.

**API stability.** Until 1.0, any 0.x minor release (0.1 → 0.2) may break the API, as Cargo's semver rules allow; patch releases (0.1.x) do not. The supported entry point is the `aegle` facade, plus `aegle-ui` and `aegle-widgets` for control libraries. The other crates are building blocks published so the facade can depend on them; they change more freely. Some APIs re-export `wgpu`, `ash` and `accesskit`, so a breaking upgrade of those dependencies is a breaking Aegle release.

## Quick start

Add the facade crate (Linux needs Fontconfig for system fonts, and libxkbcommon development files for the `wayland` backend):

```toml
[dependencies]
aegle = "0.1"
```

Describe the interface in `main.aegle` next to your `Cargo.toml`:

```text
Window {
    title: "Hello"
    Text { text: "你好，世界" }
}
```

and run it:

```rust
use aegle::prelude::*;

fn main() -> Result<()> {
    App::run_ui(aegle::ui!("main.aegle"))
}
```

The same program in plain Rust:

```rust
use aegle::prelude::*;

fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Hello")?;
    window.text("你好，世界");
    app.run()
}
```

State in plain Rust lives in signals: a binding follows the signals it reads for as long as its control lives, and input controls write the user's changes back.

```rust
let name = Signal::new(String::from("世界"));
window.text_field("").bind_text(&name);
name.bind(&window.text(""), |label, name| label.set_text(&format!("你好，{name}")));
```

Try the bundled examples with `cargo run -p aegle --example controls --release`.

## Features

Set on the `aegle` crate. The defaults are `desktop`, `sdl3` and `tiny-skia`; `desktop` is `native`, `system-fonts`, `markup`, `motion`, `effects` and `desktop-services`.

A build contains exactly one renderer. The four renderer features (`tiny-skia`, `cairo`, `vulkan`, `wgpu`) are peers and mutually exclusive: enabling two is a compile error, so a binary never carries a backend it cannot use, and `aegle::app::RENDERER` names the one it has. `desktop` holds every default except the renderer and the Linux window backend, so another renderer replaces tiny-skia with `--no-default-features --features desktop,<renderer>` on Windows and macOS, and `desktop,sdl3,<renderer>` (or `desktop,wayland,<renderer>`) on Linux, for example `cargo run -p aegle --release --no-default-features --features desktop,sdl3,cairo --example showcase`.

| Feature | Default | Purpose |
| --- | :---: | --- |
| `desktop` | ✓ | The defaults other than the renderer and the Linux window backend |
| `native` | ✓ | The native window host: Win32 on Windows, AppKit on macOS; on Linux it needs `sdl3` (the default) or `wayland` |
| `sdl3` | ✓ | Linux: windows through the distribution's `libSDL3.so.0`, loaded at run time |
| `wayland` |  | Linux: the native Wayland client instead, with layer surfaces, a drag source and text-input-v3 surrounding text |
| `tiny-skia` | ✓ | Renderer: CPU rendering with tiny-skia coverage |
| `cairo` |  | Renderer: CPU rendering with system cairo coverage (Linux), `libcairo.so.2` loaded at run time |
| `system-fonts` | ✓ | System font discovery |
| `markup` | ✓ | `ui!` macro and the markup engine, including runtime loading |
| `motion` | ✓ | Transitions, keyframe animations and springs |
| `effects` | ✓ | Image effects (`aegle::image::effects`) |
| `colrv1` |  | COLRv1 colour glyphs |
| `desktop-services` | ✓ | File dialogs, notifications, tray icon and global shortcuts (`App::desktop`) |
| `vulkan` |  | Renderer: Vulkan through ash |
| `wgpu` |  | Renderer: portable wgpu (Vulkan, Metal, Direct3D 12) |
| `jpeg` / `webp` / `gif` |  | Decode JPEG, static WebP and the first GIF frame in `aegle::image::decode` |
| `svg` |  | Static SVG images (`aegle::image::svg`) and OpenType-SVG glyphs |
| `grid` |  | Grid and stack containers, and the `Grid`/`Stack` markup components |
| `shaders` |  | Custom fragment programs (`ShaderView`, markup `Shader`), drawn by the `vulkan` and `wgpu` renderers |
| `accessibility` |  | Semantic tree plus the target's system adapter: AT-SPI on Linux (pulls in zbus), UI Automation on Windows, NSAccessibility on macOS. Implies `native` |

`native` builds only the target's platform crate, and `accessibility` only the target's adapter. On Linux `sdl3` and `wayland` are exclusive: enabling both is a compile error, and so is `native` through this facade without either, which names the rebuild command. The crate-level breakdown is in the [module guide](https://github.com/antinomie1/aegle/blob/main/docs/modules.md).

## Documentation

The developer documentation is written in Chinese:

- [API guide](https://github.com/antinomie1/aegle/blob/main/docs/developer/api.md): dependencies, applications and windows, layout, styling, events, animation, markup and embedding.
- [Control reference](https://github.com/antinomie1/aegle/blob/main/docs/developer/controls.md): every default control with screenshots of its states.
- [Controls without the facade](https://github.com/antinomie1/aegle/blob/main/docs/developer/standalone.md) (Chinese): aegle-ui, aegle-widgets, aegle-theme and aegle-motion in your own host, and custom controls.
- [Modules, dependencies and builds](https://github.com/antinomie1/aegle/blob/main/docs/modules.md), [design index](https://github.com/antinomie1/aegle/blob/main/docs/README.md) and [implementation status](https://github.com/antinomie1/aegle/blob/main/docs/implementation.md).

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/antinomie1/aegle/blob/main/LICENSE-APACHE) or [MIT license](https://github.com/antinomie1/aegle/blob/main/LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work, as defined in the Apache-2.0 license, is dual licensed as above, without any additional terms or conditions.
