# Aegle

**English** · [简体中文](README.zh-CN.md)

Aegle is a modular retained-mode GUI library for Rust 2024, built for low memory use, event-driven updates, CJK text and native input methods. Controls are created once and changed through handles; nothing redraws or polls while the UI is idle.

It is a workspace of independent crates (retained tree, Taffy layout, text and glyphs, drawing scenes, software (tiny-skia or system cairo) / Vulkan / wgpu renderers, Wayland and Win32 platform backends, controls, themes, markup) plus the `aegle` facade that ties them together. The project is in progress and not yet published to crates.io; see the [implementation status](docs/implementation.md) for what works today.

## Quick start

Add the facade crate by path or git (Linux needs libxkbcommon development files, and Fontconfig for system fonts):

```toml
[dependencies]
aegle = { path = "path/to/aegle/crates/aegle" }
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

Try the bundled examples with `cargo run -p aegle --example controls --release`.

## Features

Set on the `aegle` crate. The defaults are `native`, `tiny-skia`, `system-fonts`, `markup`, `motion`, `effects`, `colrv1` and `desktop-services`.

The four renderer features (`tiny-skia`, `cairo`, `vulkan`, `wgpu`) are peers: each compiles one backend, they may be combined, and no build treats one as "the software path". A build's default backend is the first of them it contains, in that order; any backend can be selected by name with `RendererChoice::named` and `showcase <name>`.

| Feature | Default | Purpose |
| --- | :---: | --- |
| `native` | ✓ | Native windows: Wayland on Linux, Win32 on Windows, AppKit on macOS |
| `tiny-skia` | ✓ | CPU rendering with tiny-skia coverage |
| `cairo` |  | CPU rendering with system cairo coverage (Linux), `libcairo.so.2` loaded at run time |
| `system-fonts` | ✓ | System font discovery |
| `markup` | ✓ | `ui!` macro and the markup engine, including runtime loading |
| `motion` | ✓ | Transitions, keyframe animations and springs |
| `effects` | ✓ | Image effects (`aegle::image::effects`) |
| `colrv1` | ✓ | COLRv1 colour glyphs |
| `desktop-services` | ✓ | File dialogs, notifications, tray icon and global shortcuts (`App::desktop`) |
| `vulkan` |  | Vulkan rendering through ash |
| `wgpu` |  | Portable wgpu rendering (Vulkan, Metal, Direct3D 12) |
| `accessibility` |  | Semantic tree plus the target's system adapter: AT-SPI on Linux (pulls in zbus), UI Automation on Windows, NSAccessibility on macOS. Implies `native` |

`native` builds only the target's platform crate, and `accessibility` only the target's adapter. The crate-level breakdown is in the [crate guide](docs/crates-and-examples.md).

## Documentation

The developer documentation is written in Chinese:

- [API guide](docs/developer/api.md): dependencies, applications and windows, layout, styling, events, animation, markup and embedding.
- [Control reference](docs/developer/controls.md): every default control with screenshots of its states.
- [Controls without the facade](docs/developer/standalone.md) (Chinese): aegle-ui, aegle-widgets, aegle-theme and aegle-motion in your own host, and custom controls.
- [Crates, examples and builds](docs/crates-and-examples.md), [design index](docs/README.md) and [implementation status](docs/implementation.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work, as defined in the Apache-2.0 license, is dual licensed as above, without any additional terms or conditions.
