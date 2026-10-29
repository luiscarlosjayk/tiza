# Compatibility fixtures

One file per **producer**, named `v<schema>-<app version>-<what>.tiza`, written by that version of Tiza and never edited afterwards. `qa/tests/saving/format_compatibility.rs` opens each one with the current code, saves it, reopens it and compares; it also renders it and compares against the snapshot next to it. When `SCHEMA_VERSION` grows, the test demands a fixture for the new version and a migration in `tiza_core::fs::MIGRATIONS`.

- `v1-0.0.1-ownership.tiza` — the first demo, exactly as v0.0.1 saved it (embedded code, Rust-only mark kinds).
- `v1-0.0.1-rust-kinds.tiza` — every v0.0.1 shape and the Rust kinds that are aliases today (`borrow`, `borrow_mut`, `lifetime`).
- `v1-newer-unknown-effect.tiza` — written by hand as a newer Tiza can write it: an `effect` that this version does not know (`"slide"`, `1`, and an object that is longer than the messages show), beside the two values it knows (`draw` and `fade`). The document opens, those annotations come in with no effect, and a save keeps `draw` and `fade` as they are.

- `v1-newer-unknown-move.tiza` — written by hand as a newer Tiza can write it. It has moves with an `action` that this version does not know: `teleport` in the middle of step 2, and `view` at step 5. It has an `op` that this version does not know (`swap`, the only move of step 4), and an `align` and a `typing` that it does not know. The other moves are moves that it knows. The document opens: Tiza skips those moves and uses `center` and `instant`. At step 5, a `scroll` after a skipped move misses its line. `tiza check` names the `scroll` and the move that removes the line by their numbers in the file.

- `v1-roundtrip-scroll.tiza` — written by hand: a `scroll` move with each target (`line`, `find` with `occurrence`, `annotation`) and each `align`, one of them without `align` (the default, `center`). A save writes the moves as they are in the file.

A fixture named `v1-newer-*.tiza` is a file of a newer Tiza. It does not follow the schema of this version, so `scripts/check-schema.py` expects it to fail the validation, and fails when it validates.
