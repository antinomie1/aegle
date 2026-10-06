# Aegle engineering rules

- Build bottom-up in Rust 2024. Keep RAM, allocations and idle CPU low; measure consequential performance changes instead of claiming them.
- Reuse a small dependency when it removes substantial repeated machinery. Share project logic where multiple real consumers need it. Keep modules independently useful. Add a seam for an actual backend or consumer; avoid speculative frameworks and forwarding-only crates.
- Keep each source file at most 500 lines. Split by responsibility, keep public APIs documented, and make examples executable as implementation arrives.
- Validate public inputs and FFI invariants once at their boundary. Internal code relies on established invariants; avoid redundant checks, silent fallbacks and defensive scaffolding. Use safe Rust unless a measured need or FFI requires otherwise; document every unsafe invariant.
- Preserve retained state and shared behavior across rendering, input, IME and accessibility. A backend must not silently discard unsupported required behavior.
- Keep tests in each crate’s `tests/` directory, outside `src/`. Test critical lifecycle, protocol, text and state-transition boundaries. Prefer a small integrated scenario over tests mirroring every method; run relevant formatting, checks and tests before committing.
- Commit coherent implementation milestones automatically, after verification; avoid both a single end-of-project commit and commits for trivial edits.
- Treat `docs/README.md` as the design index. Before changing ownership/modules read `docs/architecture.md` and `docs/modules.md`; before text/input work read `docs/text-input.md` and `docs/accessibility.md`; before backend work read `docs/platform-rendering.md` and `docs/resources.md`. Update affected contracts when implementation changes them.
- Keep `docs/implementation.md` factual: implemented paths, verification evidence and next integration work. Never present a stub, uncompiled target or estimated budget as working support.
