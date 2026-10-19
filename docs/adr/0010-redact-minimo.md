# ADR 0010: Minimal redact — a mask the presenter owns, on the file's own code

- Status: accepted (2026-10-19, Luis)
- Board: t-183 (phase 1: the plan; phase 2: six commits); supersedes the tracking mechanisms of t-140, t-154, t-156, t-157, t-174 and t-177 (t-168 absorbed); keeps t-117 and the generic part of t-167
- Related: [ADR 0007](0007-moves-y-pasos-con-nombre.md) §3(g) (redacts and moves), [ADR 0009](0009-politica-de-red.md) (nothing leaves the machine)

## Context

t-117 added a `redact` annotation: Tiza owns the buffer it shows, so one mask covers a span of code on the screen, every export, the recordings and what agents read. Each later card closed one more way the value could come back, and each did it by **tracking the value**: through a move that rewrote its line (t-140), pinned to the move that wrote it (t-154), into the text of other annotations, ids, titles and file names, whole or by pieces of four characters (t-157, t-174, t-177), out of every error and echo (t-167). By 2026-10-19 that was ~6,800 lines of product and ~9,600 of tests, a filter on every frame, words of the presenter's own labels masked because they shared four letters with a value, and an oracle risk in every answer that depended on the value (t-179). None of it had shipped in a release (v0.0.1 predates t-117).

Option 2 of the t-183 card: stop tracking the value and guard the mask instead.

## Decision

1. **A redact is an annotation over columns of the base code** (step 0 of its file, the main one or a linked one: decision D1). It masks from step 0 (`redact_from_step` is gone: D3). One place masks it, `redact::masked()` at the layout boundary, for the TUI and every export; `code.get`, `scene.get` and the `tiza://` resources read the same masked code. Eight `•`, plus the last `keep` characters.
2. **Only the presenter takes a mask away.** In the TUI `:redact` adds, `x`/`:del` deletes, `u` undoes, `:redact off` unmasks his own screen only, `:redact auto` stays his (D2). Agents (socket and MCP) may **add** redacts; they may not delete, narrow, move or retype one, change the text of a masked line, or put back code other than the masked copy `scene.get` hands out; `scene.clear` keeps redacts and says so. Every refusal is **one constant sentence** (`redact::AGENT_REFUSAL`), whatever the value or the request: no oracle.
3. **No move touches a masked line**, whoever adds it: a `code.patch` that would replace or delete one is refused by `move.add`, on load (D4: a document saved with such a move does not open) and by `tiza check` (`move_on_mask`); deleting a move that would make a later one hit a masked line is refused too. Inserts shift the mask with its line; a `rebind` cannot move a redact. Move targets are matched on the masked code, so a target under a mask reads like absent text.
4. **Follow and paste fail closed by the whole line** (AC6): a masked line whose text comes back identical keeps its columns wherever it moved; a changed line is masked whole (no tail), every line of a hunk that changed size too, loudly; only a line deleted with nothing in its place drops the redact.
5. **Saving**: a `.tiza` with embedded code is written with the base code (and the `views`) masked; a file reference stays a reference, the `.tiza` keeps columns only.
6. **Errors**: the generic rule of t-167 stays — a closed value (a color, a type, an op, a method) is never repeated, and serde's list of valid names is kept (`error::serde_text`); `unknown field` names the field again. A load error of a document with a redact does not quote the document's own targets (`find «…»`, a linked file's path) nor the value serde read (D7). What an agent just sent is quoted as it wrote it.
7. **Old documents** (same schema, nothing shipped): `redact_from_step` is ignored, `widened` becomes `keep: 0`, `pin: 0` or `step: 0` stay as they are, a redact at `step > 0` is carried back to the base code through the moves' inverse maps when its line is the file's own, and a redact on a line a move wrote (`pin: N > 0`, or no way back) does not open: a clear error, `redact_on_move_line` in `tiza check`.

## Threat model

It protects the **screen and everything drawn from it** (exports, GIF, `.cast`, `.tape`), the **files Tiza writes** (an embedded `.tiza` holds the masked code) and **agents that only reach Tiza through its socket or MCP**: they read masked code, cannot remove a mask, and no answer depends on the value (a `find` matches the masked code; refusals are constant). It does **not** protect copies of a value in the presenter's own texts (labels, notes, titles, ids: shown as written, his responsibility — use fake values in demos), the source file itself (it still holds the value) nor an agent that can read the file from disk. A secret split over two lines needs two redacts; `:redact auto` only knows its patterns.

## Consequences

- About −4,400 lines of product (net, measured against b30a9a1; the plan expected about −5,300 net) and −6,500 of qa tests (t-140, t-154, t-156, t-157 and t-174 test files deleted; t-167 trimmed to its generic part; new `qa/tests/t-183.rs`).
- No per-frame filter of texts and no cache of masked values. Measured on Linux x86_64, 2 CPUs, before (b30a9a1) → after: release binary 14,355,752 → 13,989,728 bytes, release archive 6,872,332 → 6,686,317 bytes (`scripts/size-check.sh`); `t054_frame_median` 9.06 → 7.47 ms (it was the one scenario over the +20 % warning of `scripts/bench.sh`), `render_txt_typescript_3000_lines` 1,316.7 → 1,242.6 ms, `tui_first_frame` 35.7 → 32.2 ms.
- A presenter who masks a value a move types in must put the value in the file instead (or not mask it); the load error says so.
- A warning when a label repeats a masked value, on the presenter's surfaces only, is a separate card (t-184, D9).
