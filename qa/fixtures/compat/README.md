# Compatibility fixtures

One file per **producer**, named `v<schema>-<app version>-<what>.codedraw`, written by that version of CodeDraw and never edited afterwards. `qa/tests/t-073.rs` opens each one with the current code, saves it, reopens it and compares; it also renders it and compares against the snapshot next to it. When `SCHEMA_VERSION` grows, the test demands a fixture for the new version and a migration in `codedraw_core::fs::MIGRATIONS`.

- `v1-0.0.1-ownership.codedraw` — the first demo, exactly as v0.0.1 saved it (embedded code, Rust-only mark kinds).
- `v1-0.0.1-rust-kinds.codedraw` — every v0.0.1 shape and the Rust kinds that are aliases today (`borrow`, `borrow_mut`, `lifetime`).
