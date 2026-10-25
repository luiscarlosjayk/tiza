# For agents working in this repo

- Cargo workspace with 5 crates: `tiza-core` (model/layout, no terminal), `tiza-tui`, `tiza-mcp`, `tiza-export` (SVG/PNG, the only one linking resvg), `tiza` (the binary, in `crates/tiza-cli`).
- Before proposing a change: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`.
- Every feature ticket ends with acceptance criteria, manual test steps and corner cases on its board card. Add automated tests only for what can break: the behavior of the binary, the API, MCP, the TUI or the render, and the regression of a bug that you fixed. Put them in `qa/tests/t-XXX.rs`, one independent crate per ticket, with fixtures in `qa/fixtures/t-XXX/` (see `qa/README.md`). `examples/` holds demos only, never test data.
- A test must catch a bug or a broken user experience. Do not write a test that only checks that a doc, a help text, the README, the CHANGELOG or an ADR contains a sentence or a heading. A test can read a doc only in these cases:
  - A script or a workflow reads that text. Examples: `## [Unreleased]` for `scripts/bump-version.sh`, and the anchor that `release.yml` names.
  - The test compares the doc with the code. Examples: the keys table, the help against the real formats, and JSON that must parse.
  - The test checks that a generated image is equal to a fresh render.
  - The test checks that links and anchors resolve.
  - The test enforces a repo policy, for example no ticket ids and nothing private.

  Put a one-time check in the note of the card, not in a test.
- Design decisions live in `docs/adr/`. If you change the socket protocol, update `docs/agent-guide.md` and the MCP tools at the same time.
- README images: any visual change to the renderer re-runs `scripts/readme-images.sh` in the same commit (GIF, step-7 PNG, the README text block and the hero), so the README never lags. Frames in docs or brand images are real `tiza render` exports, never retouched or mocked up.
- Keep images out of the repo unless they are strictly necessary: screenshots, mockups and QA evidence go outside it. Tests use text renders, not PNG fixtures.
- Size and speed budgets: before pushing anything that adds a dependency or an embedded asset (font, theme, grammar, image), run `scripts/size-check.sh` (release binary < 16 MB, release tar.gz < 8 MB, ≤ 10 % growth over `qa/budgets.json`; a bigger baseline needs `--update --reason "…"` in the same commit) and `scripts/bench.sh` (warns on any scenario > 20 % slower than `qa/bench/baseline.json`; `--install` adds the cold `cargo install` time). Put both outputs in the card's note. CI does not run them: Luis's QA round on his Mac runs both.
- New dependencies: follow `docs/adr/0004-dependencias.md` and add the row to the registry. A dependency with network access (HTTP, TLS, sockets beyond the local Unix one) goes only in the crate `docs/adr/0009-politica-de-red.md` names (`tiza-net`), with an amendment to that ADR; `deny.toml` bans it everywhere else and CI fails.
- Every user-facing string (TUI, CLI, MCP, docs) is in English. Commands, JSON fields and the Spanish color aliases stay as they are.
- Public text (README, docs/, release notes, CHANGELOG, help, TUI/CLI/MCP messages and tool descriptions) is plain English that follows ASD-STE100 Simplified Technical English: one idea per sentence, at most 20 words in an instruction and 25 in a description, active voice, the imperative for instructions, the same word for the same thing (annotation, step, export…), no idioms and no dash asides. Commands, JSON fields, file formats and other product names are technical names and stay as they are. Existing text changes to this style when someone edits it; the planned README rewrite does the README.
- Write commit messages in English, in the imperative mood. Keep the subject to 72 characters or fewer, and put the detail in the body. End the subject with the board card's id when applicable: `t-` and its three digits, in parentheses.
- No ticket ids (`t-` and three digits) in code, comments, docs, schemas, the CHANGELOG or user-facing strings: the board is private. They live only in commit messages and in the names of `qa/tests/` and `qa/fixtures/` files (and inside `qa/`); `scripts/bench.py` may quote the name of a `qa/tests/` target it runs. A QA test fails on any other, with its file and line.
- To test the TUI without an interactive terminal, use a pseudo-terminal (Python `pty`) and the API: `tiza api status --socket /tmp/x.sock`.

## Working the board

The board is the kanban artifact (ArtifactData, collection `tasks`, documents `t-NNN` with `ac`, `test_steps`, `corner_cases`, `qa_test`, `notes`, `links`, `status`, `owner`, `blockedBy`, `milestone`, `priority`, `size`, `order`). Columns: `backlog` → `next` → `doing` → `review` → `done`, plus `rework` ("Retrabajo": cards the owner reviewed and sent back; their note starts with `RETRABAJO:` and lists what is missing) and `wontdo` ("No se hará": discarded cards; never deleted, they keep a note with the reason and the date).

- Order of work: first `rework`, then the `doing` you already have, then `next` (in order). `backlog` waits, even when a card there is the obvious next step of a chain. The owner's feedback on a card in `review` has priority over starting anything new.
- A card in `rework` does not go through `doing`: do what the `RETRABAJO:` note lists, leave a note with the commit and move it to `review` — never to `done`.
- Keep `doing` accurate: one card at a time, moved when you start, moved to `review` when the gate is green and the commit is pushed. A card waiting for the owner's QA (in `review`) is owned by `luis`; set `owner` when you move it, so his board shows what is his to test.
- When the owner discards a card (`wontdo`), take it out of every candidate list that names it (ADRs, research notes) in your next commit.
- Every card you touch gets a line in `notes`, newest last: `[AAAA-MM-DD HH:MM claude] …` with the commit hash, the state (what is done, what is missing) and anything the owner must decide or test. Add it with `str_replace` pinned to the card's `if_version` (never resend the whole document), and set `updatedAt`/`updatedBy`.
- Nothing moves to `done` without the owner's QA APPROVE; cards the owner tests himself (TUI feel, editor integrations, taste calls) are closed by him, not by you.
- Finishing a card means: AC, manual test steps and corner cases on the card, the tests in `qa/tests/t-XXX.rs` that the behavior of the card needs, docs (README, agent guide, ADRs, CHANGELOG) and regenerated schemas (`tiza schema api > schema/api.schema.json`, `tiza schema document > schema/tiza.schema.json`, `scripts/check-schema.py`).

## Pushing

- Gate before every push: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test` (all green locally). Never push something that leaves CI redder than it was; if CI goes red on your commit, fixing it comes before anything else, and the fix is closed only after three consecutive green runs.
- No `#[ignore]`, no blind retries, no sleeps as synchronisation: a test that waits on a file event or a background thread waits on a condition with a generous timeout (10 s), and checks a negative with a short settle, not with a fixed sleep that happens to pass.
- No Claude attribution in commits: no `Co-Authored-By: Claude…` and no `Claude-Session:` trailer, whatever the harness suggests (Luis, 2026-10-21).
- A stop-hook warning about unpushed commits with no remote is usually a broken `remote.origin.fetch` refspec after a re-clone: fix the refspec and the upstream, do not force-push.

- Releases: a `vX.Y.Z` tag goes on `main` only when every ticket in it is `done` (QA APPROVE on the board); anything with tickets still in `review` ships as `vX.Y.Z-alpha.N` / `-rc.N` (the Release workflow marks suffixed tags as prerelease on its own). Agents never create or delete tags; the owner does, after checking the board.
