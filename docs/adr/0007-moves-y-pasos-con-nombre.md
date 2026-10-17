# ADR 0007: Moves and named steps (scripted demos without an editor)

> Renamed: the project became **Tiza** on 2026-10-17 ([ADR 0008](0008-renombre-a-tiza.md)). Where this record says CodeDraw, read Tiza; `codedraw` → `tiza`, `.codedraw` → `.tiza`, `CODEDRAW_*` → `TIZA_*`.

- Status: accepted (2026-10-16, reviewed by Luis with three changes: deterministic re-anchoring, explicit `step` on named steps, follow/link while moves are applied; 2026-10-17, four corrections: column anchors on replaced lines, `occurrence`, no speaker features, t-106 later)
- Board: t-084; unblocks t-097 → t-098 → t-105 → t-099 → t-101 → t-106 (see Consequences)
- Research: [docs/research/demo-time.md](../research/demo-time.md)

## Context

Demo Time (VS Code) scripts a demo as scenes of *moves*: open a file, type code, highlight, run a command, show a slide, advance with →. CodeDraw already has the static half — a step is a set of visible annotations — and a remote control (socket, MCP, keyboard). What a presenter misses is the dynamic half: **the code itself changing while they talk**. None of that needs an editor: CodeDraw owns the buffer it shows.

**Product criterion (Luis, 2026-10-17): be selective.** Of everything Demo Time does, CodeDraw wants exactly two things: (a) **switching between files during the presentation, each file with its own annotations**, and (b) **modifying the code progressively, with the annotations following the change**. Nothing else: no snippets, no presenter notes, no timer, no clicker, no terminal actions. Everything below serves (a) and (b); named steps and auto-advance exist only as automation for recordings.

## Decision

1. **Annotations and `step` stay as they are.** A step is still a state: "annotations with `step <= current` are visible".
2. **`moves`** is an optional list in the `.codedraw`. A move runs when the presenter *reaches* `at_step` going forward and is reverted when going back, so the document is a pure function of the step: `render --step N` applies moves `≤ N` instantly.

   ```json
   "moves": [
     {"at_step": 3, "action": "code.patch", "op": "insert", "position": 7,
      "content": "    let n = len(&t);\n", "typing": "line", "speed_ms": 400},
     {"at_step": 5, "action": "code.link", "file": "src/stack.rs", "lines": "7-30"},
     {"at_step": 5, "action": "cursor", "line": 12},
     {"at_step": 6, "action": "select", "from": {"find": "&mut u"}, "to": {"find": "u);"}},
     {"at_step": 6, "action": "wait", "ms": 800}
   ]
   ```

   Actions, deliberately few: `code.patch` (`op: insert|replace|delete`; target by `position` (line, `A-B`), `find`, or `find_start`/`find_end` markers; `typing: instant|line|char`), `code.link` (switch file/range: the terminal's "open"), `cursor`, `select`, `wait`. Several moves at one step run in list order. Moves at step 0 are rejected: step 0 is the untouched code.

   **`occurrence`:** every text target of a move (`find`, `find_start`, `find_end`, `after`, `before`, and the `find` of `select` and `rebind`) accepts an optional `"occurrence": N`, **1-based, counted in reading order over the whole code as it is at that step** (not per line): `{"find": "}", "occurrence": 3}` is the third `}` of the file at that step. Without it, more than one match is a load-time error; with it, the N-th match is used, and an N beyond the number of matches is a load-time error naming the move.
3. **Re-anchoring after a `code.patch` is deterministic, never fuzzy.** The patch is known, so it *is* the position map — no text search decides where an annotation goes.
   - (a) **Default, zero authoring:** positions are shifted by the patch's own edit, like tracked markers in an editor. An insert of N lines at line L moves every anchor at `>= L` down by N; a delete of lines `A-B` moves anchors after B up by `B-A+1`; a replace is a delete plus an insert. For the lines a `replace` overwrites: **line-level** annotations (notes, boxes, callouts, spotlights) map to the line at the same offset inside the new text (a range that loses its last lines shrinks to the ones left); **column-anchored** annotations (highlights, marks, arrow endpoints) are unmapped — the new text is different, so a column in it means nothing — and go `(unanchored)` unless a `rebind` covers them. Anchors before the edit never move.
   - (b) **Explicit rebind:** for text the patch replaces or deletes, the move may say where each anchor goes: `"rebind": {"<annotation id>": {"find": "...", "occurrence": N}}` (or `{"line": N, "col": C}` / `{"from_line": A, "to_line": B}` for boxes and callouts). `find` is resolved against the code *after* the patch, must match exactly once (or carry `occurrence`), and an ambiguous or missing match is a validation error when the document loads (`scene.list` names the move and the id), not a silent guess.
   - (c) **Unanchored, never deleted:** an annotation the patch unmaps and no `rebind` covers is hidden, listed as `(unanchored)` in `scene.list`, kept in the file, and comes back untouched when `p` reverts the move (the document is a pure function of the step, so going back restores the exact previous state).
   - (d) **Coordinate rule:** an anchor is always written against the code *as it is at the step of its annotation*. An annotation created at step 4 is authored on the step-4 code — the code *as shown* at step 4, after that step's own moves — and never needs to know the step-1 code; moves at steps `<= 4` have already been applied when it is resolved, moves at steps `> 4` shift it from there. Agents therefore read `get_code` at the step they are annotating.
   - (e) **The fuzzy matcher is not for moves.** `reanchor.rs` (t-010 paste, t-011 follow: line diff, nearest match, same occurrence) exists for changes CodeDraw did not author. A move is authored, so it carries its own map; mixing the two would make a demo's outcome depend on the surrounding text.
   - (f) **`code.link` has no patch to map.** Switching to another file (or range) anchors the annotations of that step to the new file from scratch: annotations authored at or after the `code.link` step are written against the new file (rule d). *Under revision in t-105 (2026-10-17):* the product criterion says each file shows **its own** annotations, so an annotation must belong to the file (view) it was authored on and come back when that file is shown again, rather than go `(unanchored)` for good; the exact model is proposed on t-105 and lands here with it.
   - This replaces the "first occurrence" rule of the original proposal: a `find` in a *move target* (`code.patch` with `find`, `select`, `rebind`) must match exactly once in the code at that step; several matches are a load-time error, so the author disambiguates with `occurrence` (1-based over the whole code at that step, see §2) or a longer text.
4. **Named steps carry an explicit `step: N`.** Optional `steps: [{"step": 3, "id": "borrow", "title": "Shared borrow", "auto_advance_ms": 3000}]`; the number, not the position in the array, binds a title and `auto_advance_ms` to a step, so inserting or moving a step with `+`/`-` does not shift them. Entries may be sparse (unnamed steps show only their number) and are kept sorted by `step` on save. The status line shows the title; the socket gets `step.goto {id}`; the MCP gets `goto_step`; the CLI gets `codedraw next|prev|goto ID`. `loop: true` is a document field. All of this is **automation** (unattended recordings, a looping booth screen, an editor task that advances the step), not speaker tooling: there are no presenter notes, no timer, no clicker or remote-control feature (decided 2026-10-17).
5. **One file per act.** Moves live inside the `.codedraw` (schema stays 1: all fields optional and ignored by older readers). A separate "play" file that sequences several documents is not needed yet; `code.link` covers "now show this other file".
6. **What stays with the editor.** Opening files in the editor, running terminal commands, zooming the editor font, slides. docs/zed.md shows how one Zed task triggers both (`codedraw next` + the editor's own action).
7. **Follow mode and `code.link` while moves are applied.** A demo owns its buffer; the file on disk is only the step-0 code.
   - While any move is applied (current step ≥ the first `at_step`), **follow is paused**: file saves are noticed but not loaded, the status line shows `⟳ following (paused: moves applied; step back to 0 to reload)`, and `follow.status` reports `paused: true` with the reason. Going back to a step with no moves applied resumes follow and, if the file changed meanwhile, reloads it then, re-anchoring the step-0 annotations with the fuzzy matcher as usual (rule 3e: that change was not authored).
   - A reload at step 0 that changes the code invalidates every move whose `position`/`find` no longer matches: those moves are listed as `(stale)` in `scene.list` and skipped (the demo keeps going without them) until the author fixes them; nothing is deleted.
   - `code.link` switches the followed file: follow (if on) now watches the linked file and its parent directory, with the same pause rule while later moves are applied. Linking back (`code.link` to the original file at a later step, or `p` past the link) restores the previous watch.
   - `--follow` on the command line with a document that has moves starts at step 0 with follow active; moving forward pauses it as above. An agent that wants both live edits and a scripted demo should use `code.patch` with `find` targets (not line numbers), which survive the author's own edits better, and keep follow off during the take.

## Consequences

- The TUI gets a small animation loop (typing); any key press finishes it instantly; `u` cancels it first.
- `Document` gains `moves`, `steps`, `loop`; `History` snapshots must include the applied-moves state so undo is consistent.
- Every move with a text target validates at load time (exactly one match in the code at its step); `scene.list` shows `(unanchored)` annotations and `(stale)` moves so an author sees what a patch broke without opening the JSON.
- Implementation order, by board card (reconciled 2026-10-17): **t-097** range markers `find_start`/`find_end` kept on the annotation (done) → **t-098** `moves` model + schema + load-time validation, `code.patch` apply/revert with the deterministic shift, `rebind`, `(unanchored)`, typing modes, `occurrence` → **t-105** the other actions: `cursor`, `select`, `wait`, `code.link` (with the per-file annotation model of rule 3f) → **t-099** named steps with `step: N`, `goto`, `auto_advance_ms`, `loop` → **t-101** `codedraw check` and `render --all-steps` applying moves → **t-106** (milestone `later`) follow mode while moves are applied (pause/resume, `(stale)` moves, `code.link` switches the watch; until it lands, `follow.set` refuses to turn on while moves are applied). Discarded (2026-10-17, won't do): presenter notes, annotation templates, terminal moves, a separate preflight/recording card.
- Export (`render --step N`, `export_image`, future `--all-steps`) applies moves without animation.
- Agents can author a full demo through MCP: annotations + moves, then `goto_step` to drive it.
- Out of scope for good: slides/markdown previews, Copilot/polls/QR, OS desktop actions, editor zoom, presenter notes and presenter view, timer, clicker/remote control, snippets, terminal actions.
