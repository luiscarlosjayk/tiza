# For agents working in this repo

- Cargo workspace with 5 crates: `codedraw-core` (model/layout, no terminal), `codedraw-tui`, `codedraw-mcp`, `codedraw-export` (SVG/PNG, the only one linking resvg), `codedraw-cli` (the `codedraw` binary).
- Before proposing a change: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`.
- Every feature ticket ends with: acceptance criteria, manual test steps and corner cases on its board card, and `qa/tests/t-XXX.rs` (one independent crate per ticket, fixtures in `qa/fixtures/t-XXX/`; see `qa/README.md`). `examples/` holds demos only, never test data.
- Design decisions live in `docs/adr/`. If you change the socket protocol, update `docs/agent-guide.md` and the MCP tools at the same time.
- New dependencies: follow `docs/adr/0004-dependencias.md` and add the row to the registry.
- Every user-facing string (TUI, CLI, MCP, docs) is in English. Commands, JSON fields and the Spanish color aliases stay as they are.
- Commits in Spanish, imperative mood, with the board task id when applicable (`t-022`).
- To test the TUI without an interactive terminal, use a pseudo-terminal (Python `pty`) and the API: `codedraw api status --socket /tmp/x.sock`.
- Releases: a `vX.Y.Z` tag goes on `main` only when every ticket in it is `done` (QA APPROVE on the board); anything with tickets still in `review` ships as `vX.Y.Z-alpha.N` / `-rc.N` (the Release workflow marks suffixed tags as prerelease on its own). Agents never create or delete tags; the owner does, after checking the board.
