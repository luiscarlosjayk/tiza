# 0004 · Dependency policy

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
| rmcp | 3.5.1 | 2026-10 | Apache-2.0 | Official MCP SDK; frequent releases; 3.x API |
| schemars | 1.2 | 2026-10 | MIT | Required by rmcp 3.x |
| tokio | 1.53 | 2026-10 | MIT | Only in `codedraw-mcp` |
| clap | 4.6 | 2026-09 | MIT/Apache-2.0 | derive |
| serde / serde_json | 1.0.229 / 1.0.154 | 2026-07 / 2026-10 | MIT/Apache-2.0 | |
| thiserror | 2 | — | MIT/Apache-2.0 | |
| unicode-width / unicode-segmentation | 0.2.2 / 1.13.3 | 2025-10 / 2026-06 | MIT/Apache-2.0 | Anchors per grapheme, width per cell |
| anyhow | 1 | — | MIT/Apache-2.0 | Only in binaries |

Evaluated and **postponed**: `notify` 8 (file watching; Zed saves with tmp+rename, watch the directory), `arboard` (in-app clipboard; `pbpaste | codedraw` covers macOS), `resvg` (PNG; requires embedding fonts), `insta`/`proptest` (will come with the snapshot tests).

### Recorded exceptions

- `bincode 1.3.3` (transitive, `syntect` → `dump-load`): RUSTSEC-2025-0141 "unmaintained". Its team declared 1.3.3 complete. It only deserializes the precompiled grammars embedded in the binary; it never touches external data. Ignored in `deny.toml`; revisit when `syntect` changes its dump format.
- `yaml-rust` (RUSTSEC-2024-0320) was avoided by disabling `yaml-load` in `syntect`: we do not load YAML grammars at runtime.
