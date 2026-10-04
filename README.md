# Aegle

A modular retained-mode GUI library in Rust 2024, focused on low memory use,
event-driven updates, CJK and native input methods. GPU and software rendering
are planned as independently selectable backends.

Implementation is in progress: compact shared types, retained trees and Taffy
layout work today. Native windows, text/IME and rendering are not implemented yet.
See the [implementation status](docs/implementation.md) and [design](docs/README.md).

```sh
cargo test --workspace --all-features
cargo run -p aegle-layout --example retained --release
cargo doc --workspace --all-features --no-deps
```

Independent crates: `aegle-types` (no_std geometry/colors), `aegle-core` (retained
state with no third-party dependencies), and `aegle-layout` (Taffy over that tree).
The example is a headless layout demonstration, not a working GUI application.

Licensed under [LGPL-3.0-only](LICENSE); the incorporated GPLv3 text is in [COPYING](COPYING).
