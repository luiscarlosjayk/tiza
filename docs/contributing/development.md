# Development

Run these commands in a clone of the repository.

```sh
cargo test                                   # core, TUI (incl. socket), commands and the per-ticket QA tests (qa/)
cargo clippy --all-targets -- -D warnings
cargo run -- examples/ownership.rs
scripts/size-check.sh                        # release binary < 16 MB, download < 8 MB, ≤ 10 % over qa/budgets.json
scripts/bench.sh                             # timings vs qa/bench/baseline.json (warns > 20 % slower); --install adds a cold `cargo install`
```

Crates: `tiza-core` (model, anchors, layout, rendering, protocol; no terminal dependencies), `tiza-tui` (ratatui + crossterm + socket server), `tiza-mcp` (MCP bridge with [rmcp](https://github.com/modelcontextprotocol/rust-sdk)), `tiza-export` (SVG/PNG with [resvg](https://github.com/linebender/resvg) and embedded fonts), `tiza` (the binary, in `crates/tiza-cli`). Decisions in [docs/adr](../adr). Dependency policy (maintained crates only, `cargo-deny` in CI): [docs/adr/0004-dependencias.md](../adr/0004-dependencias.md).

`tiza-core`, `tiza-tui`, `tiza-mcp` and `tiza-export` are on crates.io only so that `cargo install tiza` works: they have no stable API.

Inside a checkout, `rust-toolchain.toml` pins Rust 1.99.1 (with rustfmt and clippy) so local builds, the gate, CI and the release workflow use the same toolchain (the workflows install it from that file); 1.88 stays the minimum supported version.
