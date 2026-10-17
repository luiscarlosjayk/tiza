# 0004 · Dependency policy

> Renamed: the project became **Tiza** on 2026-10-17 ([ADR 0008](0008-renombre-a-tiza.md)). Where this record says CodeDraw, read Tiza; `codedraw` → `tiza`, `.codedraw` → `.tiza`, `CODEDRAW_*` → `TIZA_*`.

**Status:** accepted (2026-10-15)

## Decision

We are selective with crates: only those that are still maintained get in. Criteria per crate:

- Last release within the last 12 months **or** a repo with recent activity and attended issues.
- More than one maintainer or an organization behind it (bus factor), or a small crate we could absorb.
- Compatible license (MIT, Apache-2.0, BSD, ISC, Zlib, CC0, MPL-2.0; list in `deny.toml`).
- No unnecessary C dependencies (e.g. `syntect` with `default-fancy` instead of oniguruma).
- We prefer `std` when it suffices (Unix socket, threads, channels) over an async runtime.

Tooling: `cargo-deny` in CI (licenses, advisories, sources, duplicates), versioned `Cargo.lock`, `--locked` in releases. Every new crate is recorded here with its evaluation date.

## Registry (2026-10-15)

| Crate | Version | Last release | License | Notes |
|---|---|---|---|---|
| ratatui / ratatui-core | 0.30.2 / 0.1.2 | 2026-06 | MIT | MSRV 1.88; crossterm 0.29 by default |
| crossterm | 0.29.0 | 2025-04 | MIT | Active repo; `bracketed-paste`, `use-dev-tty` features |
| syntect | 5.3.0 | 2025-09 | MIT | `default-fancy` (pure Rust) |
| two-face | 0.5.2 | 2025-12 | MIT/Apache-2.0 | bat's grammars (TypeScript, TOML…) |
| regex | 1.13 | 2026-09 | MIT/Apache-2.0 | Already in the tree through syntect; direct dependency of `tiza-core` since t-117 (redact patterns, 2026-10-18) |
| rmcp | 3.5.1 | 2026-10 | Apache-2.0 | Official MCP SDK; frequent releases; 3.x API |
| schemars | 1.2 | 2026-10 | MIT | Required by rmcp 3.x |
| tokio | 1.53 | 2026-10 | MIT | Only in `codedraw-mcp` |
| clap | 4.6 | 2026-09 | MIT/Apache-2.0 | derive |
| serde / serde_json | 1.0.229 / 1.0.154 | 2026-07 / 2026-10 | MIT/Apache-2.0 | |
| thiserror | 2 | — | MIT/Apache-2.0 | |
| unicode-width / unicode-segmentation | 0.2.2 / 1.13.3 | 2025-10 / 2026-06 | MIT/Apache-2.0 | Anchors per grapheme, width per cell |
| anyhow | 1 | — | MIT/Apache-2.0 | Only in binaries |
| resvg / usvg / tiny-skia / fontdb | 0.48.1 / 0.48.1 / 0.12 / 0.24 | 2026-10 | MIT/Apache-2.0, BSD-3-Clause (tiny-skia) | Only in `codedraw-export`; `text` + `system-fonts` features, no `raster-images` |
| schemars | 1.2 | 2026-10 | MIT | JSON Schema of the document and the API (`codedraw-core`), shared with rmcp |
| jsonschema | 0.59.0 | 2026-10 | MIT | dev-dependency of `codedraw-qa` (validates socket replies against `schema/`) |
| insta | 1.49.0 | 2026-09 | Apache-2.0 | dev-dependency of `codedraw-qa` (render snapshots) |
| proptest | 1.12.0 | 2026-09 | MIT/Apache-2.0 | dev-dependency of `codedraw-core` (property tests) |
| base64 | 0.23.1 | 2026-09 | MIT/Apache-2.0 | `@font-face` data URIs in SVG |
| notify | 8.2.0 | 2026-02 | CC0-1.0 (inotify: ISC) | Only in `codedraw-tui`, follow mode (t-011): watches the source file's directory, debounce in our code (no `notify-debouncer-*`) |

Evaluated and **postponed**: `arboard` (in-app clipboard; `pbpaste | codedraw` covers macOS). `notify` (adopted for follow mode: Zed and most editors save with tmp+rename, so the *directory* is watched and events are matched by file name; the 120 ms debounce and the empty-write deferral live in `codedraw-tui/src/follow.rs`, so no debouncer crate is needed), `resvg`, `proptest` and `insta` were adopted later (rows above).

### Recorded exceptions

- `bincode 1.3.3` (transitive, `syntect` → `dump-load`): RUSTSEC-2025-0141 "unmaintained". Its team declared 1.3.3 complete. It only deserializes the precompiled grammars embedded in the binary; it never touches external data. Ignored in `deny.toml`; revisit when `syntect` changes its dump format.
- `yaml-rust` (RUSTSEC-2024-0320) was avoided by disabling `yaml-load` in `syntect`: we do not load YAML grammars at runtime.
