# ADR 0007: Moves and named steps (scripted demos without an editor)

- Status: proposed (2026-10-16), to be accepted when t-098/t-099 land
- Board: t-084; unblocks t-097 → t-098 → t-099 → t-100 → t-101 (t-102 later)
- Research: [docs/research/demo-time.md](../research/demo-time.md)

## Context

Demo Time (VS Code) scripts a demo as scenes of *moves*: open a file, type code, highlight, run a command, show a slide, advance with →. CodeDraw already has the static half — a step is a set of visible annotations — and a remote control (socket, MCP, keyboard). What a presenter misses is the dynamic half: **the code itself changing while they talk**, plus presenter notes and named steps a clicker can jump to. None of that needs an editor: CodeDraw owns the buffer it shows.

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
3. **Re-anchoring.** After a `code.patch`, every annotation whose anchor carries a `find` text is re-resolved (first occurrence, on its original line first); anchors without text are shifted by the patch's line delta. An annotation whose text no longer exists is **hidden**, listed as `(unanchored)` in `scene.list`, and comes back when the text does. It never crashes a render.
4. **Named steps.** Optional `steps: [{"id": "borrow", "title": "Shared borrow", "auto_advance_ms": 3000, "notes": "…"}]`, indexed by position (`steps[0]` is step 1). The status line shows the title; the socket gets `step.goto {id}` and `scene.notes`; the MCP gets `goto_step`, `get_notes`, `set_notes`; the CLI gets `codedraw next|prev|goto ID` for clickers. `loop: true` and `timer_min` are document fields.
5. **One file per act.** Moves live inside the `.codedraw` (schema stays 1: all fields optional and ignored by older readers). A separate "play" file that sequences several documents is not needed yet; `code.link` covers "now show this other file".
6. **What stays with the editor.** Opening files in the editor, running terminal commands, zooming the editor font, slides. docs/zed.md shows how one Zed task triggers both (`codedraw next` + the editor's own action).

## Consequences

- The TUI gets a small animation loop (typing); any key press finishes it instantly; `u` cancels it first.
- `Document` gains `moves`, `steps`, `loop`, `timer_min`; `History` snapshots must include the applied-moves state so undo is consistent.
- Export (`render --step N`, `export_image`, future `--all-steps`) applies moves without animation.
- Agents can author a full demo through MCP: annotations + moves + notes, then `goto_step` to drive it.
- Out of scope for good: slides/markdown previews, Copilot/polls/QR, OS desktop actions, editor zoom.
