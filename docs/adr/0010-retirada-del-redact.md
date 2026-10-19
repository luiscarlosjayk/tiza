# ADR 0010: The redact feature is withdrawn

- Status: accepted (2026-10-19, Luis)
- Board: t-183 (phase 1: a plan to shrink the feature; phase 2: eight local commits that did; then this decision). Cards t-117, t-135, t-140, t-154, t-156, t-157, t-167 (its redact part), t-174, t-177 and t-183 itself are the history of the feature; t-168, t-175 and t-179 were its open cards
- Related: [ADR 0007](0007-moves-y-pasos-con-nombre.md) §3(g) (what moves did with masks), [ADR 0009](0009-politica-de-red.md) (its «masks first» rule, now «what is sent is what is rendered»)

## Context

t-117 (2026-10-17) added a `redact` annotation: Tiza owns the buffer it shows, so one mask could cover a span of code on the screen, every export, the recordings and what agents read, while the file on disk kept the value. It was a nice-to-have for one case: showing a config file with a real token in it without the token reaching the recording.

What grew from it, in two days:

- Each review found a new way for the value to come back, and each card closed it by **tracking the value** somewhere else: through a move that rewrote its line (t-140), pinned to the move that wrote it (t-154), into the text of other annotations, ids, titles and file names, whole or by pieces of four characters (t-157, t-174, t-177), out of every error and echo (t-167), in the alert rows of the status bar (t-135, t-156). By 2026-10-19 that was ~6,800 lines of product and ~9,600 of tests, a filter on every frame, and an oracle risk in every answer that depended on the value (t-179).
- t-183 phase 1 planned a minimal redact (a mask over columns of the file's own code that only the presenter removes; agents may only add one; no move touches a masked line; follow masks a changed line whole; nothing tracks the value). Phase 2 built it in six commits and removed ~4,400 lines of product. Two adversarial reviews then found, and closed, seven more holes (path aliases of a linked file, a reload under a move, the length of a masked line in errors, a one-character oracle in `find`, the linked file's name in `check`), and the fix for the reload opened the next one: a reload refused because a move would rewrite the masked line, followed by `:w`, saved the old columns over the new file, and the next `render` or TUI showed the new value in clear without a word.

Every version of the feature has had the same shape: a mask that is right as long as the code under it does not move, and a growing list of ways the code moves (moves, reloads, pastes, aliases, puts) that each need their own rule, test and review. Luis's decision (2026-10-19): «I only wanted a user to be able to show a secret masked; if this is complicating the release for a nice-to-have, we can remove the whole feature for now and move on.»

## Decision

1. **The redact feature is withdrawn, whole.** No `Shape::Redact`, no `:redact` (by hand, `off`, `auto`), no `r` on a selection, no mask in the layout or in any export, no `redact` tool in the MCP nor `type: "redact"` over the socket, no guard on moves over masked lines, no masked save when embedding, no whole-line masking on follow or paste, no elision of the document's own targets in errors, no alert rows above the status bar. The `regex` dependency goes with it (only the auto patterns used it).
2. **What stays** is the generic part the cards left: an error never repeats a closed value (a color, a type, an op, a method) and keeps serde's list of valid names (t-167); a `code.link` or an annotation's `file` that is an alias of a file the document shows (`sub/../x`, `../dir/x`, an absolute path, a symlink) is that file's view (t-183 review); `ViewNotFound` for a file that is no view.
3. **A `.tiza` that still carries `redact` annotations does not open** — in the TUI, in `tiza render`, in `tiza check` (finding `redacts`) and through `scene.put` — with one sentence: `this document uses redacts, which this version no longer supports; remove the `redact` annotations from the sidecar`. Its code is never shown in clear without a word. Nothing with redacts shipped in a release (v0.0.1 predates t-117), so no migration is written.
4. **The advice replaces the feature**: do not put real secrets in what you record; use demo values. The README says so in two lines; the agent guide tells an agent that reads a real secret in the code to say so before anything is recorded and never to repeat it in a note or a label.

## What it would take to bring it back

Only if a user asks for it, and as one card with these in its acceptance criteria from the start, not added review by review:

- **A mask bound to the text, not to columns.** A redact must remember a fingerprint of the file it masks (a hash of the line, or of the whole file) and fail closed on open: a sidecar whose mask no longer matches the file does not show the file until the presenter looks at it with the mask off on purpose. That is the hole every version fell into (reloads, pastes, an edit with Tiza closed, a reload refused under a move).
- **One code path for every way the code changes**: reload, paste, `scene.put`, a move, an alias of the file. Each either keeps the mask verified or refuses the change; none drops a mask silently or keeps a document that will not reopen.
- **No answer that depends on the value**: a constant refusal for agents, targets matched on the masked code, no line length, no file name, no number of the request echoed on a document with redacts (the oracles of t-167, t-179 and the two reviews of t-183).
- A canary test that opens every surface (screen, exports, recordings, every socket method and MCP tool, errors, `panics.log`) with a value that is nowhere else and asserts no piece of it leaves.

## Consequences

- Measured on Linux x86_64, 2 CPUs. Lines (product = `crates/**/*.rs` outside `mod tests`; qa = `qa/tests/*.rs`): before the feature's last version (b30a9a1) 30,151 product / 29,833 qa; after t-183 phase 2 and its reviews (b390240) 26,174 / 24,830; after the withdrawal 23,729 / 21,463. Release binary (`scripts/size-check.sh`): 14,355,752 bytes at b30a9a1, 13,989,728 at the end of phase 2, 13,751,384 now (+3.7 % over the 7f71f9f baseline; the archive 6,571,198 bytes, +3.0 %). `scripts/bench.sh` (noisy on this machine, two runs): `t054_frame_median` 9.06 ms at b30a9a1 → 7.47 at the end of phase 2 → 8.82 / 7.09 now; `tui_first_frame` 35.7 → 32.2 → 34.4 / 31.3 ms; `render_gif_example` 331.5 → 321.7 → 306.8 / 309.0 ms.
- Scenario 6 of `docs/demos/scenarios.md` works on a file with demo values. The research notes (`docs/research/mcp-context.md`, `demo-time.md`) keep their history of the feature as it was measured.
- ADR 0007 §3(g) and ADR 0009 keep one line each on what they said while the feature existed.
