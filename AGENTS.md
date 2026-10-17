# For agents working in this repo

- Cargo workspace with 5 crates: `tiza-core` (model/layout, no terminal), `tiza-tui`, `tiza-mcp`, `tiza-export` (SVG/PNG, the only one linking resvg), `tiza` (the binary, in `crates/tiza-cli`).
- Before proposing a change: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`.
- Every feature ticket ends with: acceptance criteria, manual test steps and corner cases on its board card, and `qa/tests/t-XXX.rs` (one independent crate per ticket, fixtures in `qa/fixtures/t-XXX/`; see `qa/README.md`). `examples/` holds demos only, never test data.
- Design decisions live in `docs/adr/`. If you change the socket protocol, update `docs/agent-guide.md` and the MCP tools at the same time.
- New dependencies: follow `docs/adr/0004-dependencias.md` and add the row to the registry. A dependency with network access (HTTP, TLS, sockets beyond the local Unix one) goes only in the crate `docs/adr/0009-politica-de-red.md` names (`tiza-net`), with an amendment to that ADR; `deny.toml` bans it everywhere else and CI fails.
- Every user-facing string (TUI, CLI, MCP, docs) is in English. Commands, JSON fields and the Spanish color aliases stay as they are.
- Commits in Spanish, imperative mood, with the board task id when applicable (`t-022`).
- To test the TUI without an interactive terminal, use a pseudo-terminal (Python `pty`) and the API: `tiza api status --socket /tmp/x.sock`.

## Working the board

The board is the kanban artifact (ArtifactData, collection `tasks`, documents `t-NNN` with `ac`, `test_steps`, `corner_cases`, `qa_test`, `notes`, `links`, `status`, `owner`, `blockedBy`, `milestone`, `priority`, `size`, `order`). Columns: `backlog` → `next` → `doing` → `review` → `done`, plus `rework` ("Retrabajo": cards the owner reviewed and sent back; their note starts with `RETRABAJO:` and lists what is missing) and `wontdo` ("No se hará": discarded cards; never deleted, they keep a note with the reason and the date).

- Order of work: first `rework`, then the `doing` you already have, then `next` (in order). `backlog` waits, even when a card there is the obvious next step of a chain. The owner's feedback on a card in `review` has priority over starting anything new.
- A card in `rework` does not go through `doing`: do what the `RETRABAJO:` note lists, leave a note with the commit and move it to `review` — never to `done`.
- Keep `doing` accurate: one card at a time, moved when you start, moved to `review` when the gate is green and the commit is pushed. A card waiting for the owner's QA (in `review`) is owned by `luis`; set `owner` when you move it, so his board shows what is his to test.
- When the owner discards a card (`wontdo`), take it out of every candidate list that names it (ADRs, research notes) in your next commit.
- Every card you touch gets a line in `notes`, newest last: `[AAAA-MM-DD HH:MM claude] …` with the commit hash, the state (what is done, what is missing) and anything the owner must decide or test. Add it with `str_replace` pinned to the card's `if_version` (never resend the whole document), and set `updatedAt`/`updatedBy`.
- Nothing moves to `done` without the owner's QA APPROVE; cards the owner tests himself (TUI feel, editor integrations, taste calls) are closed by him, not by you.
- Finishing a card means: AC, manual test steps and corner cases on the card, `qa/tests/t-XXX.rs`, docs (README, agent guide, ADRs, CHANGELOG) and regenerated schemas (`tiza schema api > schema/api.schema.json`, `tiza schema document > schema/tiza.schema.json`, `scripts/check-schema.py`).

## Pushing

- Gate before every push: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test` (all green locally). Never push something that leaves CI redder than it was; if CI goes red on your commit, fixing it comes before anything else, and the fix is closed only after three consecutive green runs.
- No `#[ignore]`, no blind retries, no sleeps as synchronisation: a test that waits on a file event or a background thread waits on a condition with a generous timeout (10 s), and checks a negative with a short settle, not with a fixed sleep that happens to pass.
- Commit trailers: `Co-Authored-By: <agent>` and the `Claude-Session:` URL the harness gives you, every commit.
- A stop-hook warning about unpushed commits with no remote is usually a broken `remote.origin.fetch` refspec after a re-clone: fix the refspec and the upstream, do not force-push.

- Releases: a `vX.Y.Z` tag goes on `main` only when every ticket in it is `done` (QA APPROVE on the board); anything with tickets still in `review` ships as `vX.Y.Z-alpha.N` / `-rc.N` (the Release workflow marks suffixed tags as prerelease on its own). Agents never create or delete tags; the owner does, after checking the board.
