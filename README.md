# Aegle

A modular retained-mode GUI library in Rust 2024, designed for low memory use,
event-driven updates, CJK text and native input methods.

Implementation is in progress. See the [design](docs/README.md) and the
[implementation status](docs/implementation.md) for supported paths and checks.

The library is being built bottom-up: compact retained state, layout, text and
rendering first, followed by native platforms and the application interface.
