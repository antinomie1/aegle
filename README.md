# Aegle

A modular retained-mode GUI library in Rust 2024, focused on low memory use,
event-driven updates, CJK and native input methods. GPU and software rendering
are planned as independently selectable backends.

Implementation is in progress: compact shared types, retained trees, Taffy
layout, drawing records and software rasterization work today. Native windows,
text/IME, accessibility and GPU backends are not implemented yet.
See the [implementation status](docs/implementation.md) and [design](docs/README.md).

```sh
cargo test --workspace --all-features
cargo run -p aegle-layout --example retained --release
cargo run -p aegle-render-software --example software_scene --release
cargo doc --workspace --all-features --no-deps
```

Independent crates: `aegle-types` (no_std geometry/colors), `aegle-core` (retained
state with no third-party dependencies), `aegle-layout` (Taffy over that tree),
`aegle-scene` (validated drawing records), and `aegle-render-software` (borrowed
framebuffers, linear-light compositing and bounded clip masks).
The examples are headless demonstrations, not working GUI applications.
The software example writes `target/aegle-software.png`; its PNG encoder is a
development dependency and does not enter the renderer's normal dependency tree.

Licensed under [LGPL-3.0-only](LICENSE); the incorporated GPLv3 text is in [COPYING](COPYING).
