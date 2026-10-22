# ADR 0008: Rename CodeDraw to Tiza

- Status: accepted (2026-10-17, decided by Luis)
- Project document: `claude/adr-006-renombre-tiza.md` (numbered 0008 here: 0006 and 0007 already existed in `docs/adr/`)

## Context

"CodeDraw" describes what the tool does, but it is generic. The tools this project draws on (herdr, ratatui, tuios, excalidraw) have proper names that stick. There is also an unrelated Java library of the same name (Krassnig).

Renaming is cheap now: v0.0.1 had only been published as a prerelease a few hours earlier, nothing is on crates.io, and the name had not been reserved anywhere.

## Options considered

Checked on 2026-10-17 against the crates.io and npm APIs.

| Name | crates.io | npm | Notes |
|---|---|---|---|
| gloss | taken | taken | OpenGL layer, v0.0.0, untouched since 2021; crates.io never frees names. |
| rubric | taken | taken | Grading tool, untouched since 2020. |
| scrawl | taken | taken | Opens the user's editor; ~72k downloads, same niche (terminal). |
| marginalia | taken | taken | Active (lalrpop). |
| patina | taken | taken | Active (UEFI). |
| overdraw | free | taken | npm: a WebGL GPU-leak detector (Aug 2026). |
| codechalk | free | taken | `code-chalk` and `chalkcode` free on both. Discarded for how it sounds. |
| **tiza** | **free** | taken | `tizas`, `tizarr` and `tiza-rs` free on both. `tiza-core`, `tiza-tui`, `tiza-mcp`, `tiza-export` free on crates.io (checked when the rename was applied). |

## Decision

The project is called **Tiza** (Spanish for *chalk*: what you draw with on a blackboard).

Writing convention (Luis):

- **Tiza**, capitalised, everywhere people read: README, docs, wordmark, TUI texts, thumbnails, video.
- **`tiza`**, lowercase, in the CLI, the crate and the binary.
- Wordmark: `╭─ Tiza ─▶`. The one-cell icon `╭▶` does not change.

CodeDraw remains the former name of the v0.0.1 demo.

### What changed in the repo

- Crates: `codedraw-core|tui|mcp|export` → `tiza-core|tui|mcp|export`; `codedraw-cli` → package `tiza` (binary `tiza`, directory `crates/tiza-cli`); QA crate `tiza-qa`. The library crates stay publishable so that `cargo publish --dry-run -p tiza` passes.
- Environment: `CODEDRAW_SOCKET` → `TIZA_SOCKET`, `CODEDRAW_SESSION` → `TIZA_SESSION`, `CODEDRAW_GIT_SHA[_OVERRIDE]` → `TIZA_GIT_SHA[_OVERRIDE]` (build), `CODEDRAW_BIN` → `TIZA_BIN` (scripts). Session sockets live under `tiza/<session>.sock`.
- Sidecar `.codedraw` → `.tiza` (the `schema` field is an integer and does not change); JSON Schema files `schema/tiza.schema.json` and `schema/api.schema.json` with the new `$id`; examples and QA fixtures renamed.
- MCP: the server announces itself as `tiza`; resources `tiza://scene|code|render|render.md`. Tool names were never prefixed.
- Fonts: the Libron subset is now *Tiza Serif* (`TizaSerif-*.ttf`, name table rewritten; still OFL with Reserved Font Name Libron) and the DejaVu subset *Tiza Symbols*.
- README title `╭─ Tiza ─▶` and subtitle; TUI help overlay carries the wordmark; agent guide, Zed docs, CI and release workflow (`tiza-vX.Y.Z-<target>.tar.gz`), `.gitignore`.
- ADRs 0001–0007 keep their text (they are history) with a note at the top.

### Luis's part (needs his account)

- Reserve `tiza` on crates.io (the first publish reserves the name) before announcing anything.
- GitHub repo renamed to `luiscarlosjayk/tiza` (done 2026-10-17; GitHub redirects the old URL).
- Delete the CodeDraw prerelease and tag `v0.0.1`, then re-tag v0.0.1 on `main` after this change so the first release ships as Tiza.
- Rename the board artifact to "Tiza Board".

## Consequences

**For**

- Short (4 letters) and comfortable to type: `tiza example.rs`.
- Keeps Excalidraw's blackboard metaphor without being literal.
- Free on crates.io, the registry that matters for `cargo install`.
- A nod to the channel and its Spanish-speaking audience; it sets the tool apart.

**Against**

- Not descriptive for English speakers (roughly "TEE-sa"). It leans on the subtitle — *a Rust TUI to draw on top of code in the terminal* — in the README title, the crate description and the videos.
- Searching "tiza" returns physical chalk; pair it with "Rust" or "terminal".
- `tiza` exists on npm. Irrelevant for a Rust crate, relevant if an npm wrapper ever appears.

**Unverified**: GitHub (no `tiza` TUI/Rust project found by web search, not conclusive), domains and trademarks.

## Open decision, closed

Old `.codedraw` files are **not** read (Luis, 2026-10-17): v0.0.1 as CodeDraw is withdrawn, so the only such files are Luis's own tests, renamed by hand.
