# Demo Time, mapped to Tiza (t-083)

Research note, 2026-10-16. Sources: demotime.show (getting started, JSON schema, actions/text, references/api, features/presenter-view). Demo Time is licensed under the Apache License 2.0 (per its repository README); nothing of it is copied here, only its vocabulary where it helps.

## Decisions (2026-10-17)

- **Be selective.** Tiza takes two things from Demo Time: (a) switching between files during the presentation, each with its own annotations (t-105, `code.link`), and (b) modifying the code progressively with the annotations following the change (t-098, `code.patch`). Tiza's focus is showing code and annotations, with transitions that look good on video.
- **Out of scope:** presenter notes, presenter view / second pane, timer, clicker and remote control, snippets/templates, terminal actions. The cards t-032, t-087, t-088 and t-100 were deleted; t-102 (templates), t-085 (superseded by t-098), t-086 (terminal moves) and t-089 (duplicate of t-101 + t-051) are `wontdo`.
- **Added:** t-105 (`cursor`, `select`, `wait`, `code.link`) and t-106 (follow mode while moves are applied, milestone `later`).
- Anything in the table below that is not a board card is **not adopted** unless Luis asks for it. The table is kept as a dated record.

## What Demo Time is

A VS Code extension (also Cursor, Windsurf, Codespaces) that runs **scripted code demos**: a `.demo/*.json|yml` file lists *scenes* made of *moves* (schema v3; v1–2 said *demos* and *steps*). Pressing → runs the next move: open a file, insert text with a typing effect, highlight a range, run a terminal command, show a slide, wait, zoom. It ships slides (markdown with layouts/themes), presenter view with notes, a timer, a local HTTP API (`/api/next`, `/api/runById`, `/api/screenshot`…) used by a remote-control PWA, Stream Deck-style clickers and an MCP server, plus recording/export to video, a snippet gallery, and "EngageTime" polls.

The important observation: **everything Demo Time does happens inside the editor's UI** (decorations, editors, terminals, webviews). That is both its strength (the demo *is* the real editor) and why it is VS Code-only.

## The schema, condensed

```
title, description, version (1|2|3), timer (min), loop
scenes[]: id, title, description, disabled, autoAdvanceAfter (s), icons{start,end}, notes{path, showOnTrigger}, moves[]
moves[]:  action + action-specific fields, disabled
position: "10" | "10:20" | "10,5" | "10,5:20,10" | "start" | "end"
          or startPlaceholder/endPlaceholder = comment markers in the file ("// Start of demo1")
```

Actions (90+), by family: **file** (open, create, rename, delete, close, copy, move, applyPatch, markdownPreview, imagePreview, openSlide), **text** (insert, replace, write, delete, highlight, selection, unselect, positionCursor, format, save; typing modes instant / line-by-line / character-by-character / hacker-typer), **setting** (setSetting, setTheme, zoomIn/Out/Reset, zen mode, presentation view, backup/restore settings), **terminal** (openTerminal, executeTerminalCommand, executeScript, sendKeybinding, closeTerminal; waitForMessage), **time** (waitForTimeout, waitForInput, pause), **vscode** (executeVSCodeCommand, showInfoMessage, setState, openWebsite), **snippet** (reusable parametrized step lists), **copilot** (open/ask/edit/agent chat), **interactions** (typeText, pressEnter/Tab/arrows/Escape/Backspace/Delete, clipboard), **os/desktop** (macos.* focus mode, menubar, dock, caffeine, hide desktop icons), **demotime** (runDemoById, showQR/hideQR), **engagetime** (polls).

Highlight specifics worth noting: whole-line highlight has a border; character-range highlight has none; `highlightBlur` / `highlightOpacity` dim everything else (the same idea as Tiza's spotlight); `zoom` scales the editor font for that step.

## What transfers to a terminal tool, and what does not

| Demo Time | Editor-bound? | Tiza today | Verdict |
|---|---|---|---|
| Scenes → moves, run in order with → | No | Steps (`step` per annotation, `n`/`p`) | **Exists.** Our step is a *state* (what is visible); theirs is an *action*. A Tiza "move" would be a state transition, see proposal. |
| `highlight` range, whole line / chars | No | `highlight`, `box`, marks | **Exists** (richer: labels, legends, colors). |
| `highlightBlur` / `highlightOpacity` | No | `spotlight` | **Exists.** |
| Placeholders (`// Start of demo1`) to locate ranges | No | `find` text anchors | **Exists** (find is per line; range-by-two-markers is a small add: t-097). |
| `insert` / `replace` with typing modes | No (needs a buffer we own) | `replace_code` (paste) only | **Planned**: code *diffs* as steps (show v1, then v2 with the added lines typed in). This is the biggest gap and the most valuable: "explain, then evolve the code". |
| `positionCursor`, `selection` | No | Cursor and selection exist for authoring, not as steps | **t-105** (`cursor`, `select` moves). |
| `zoomIn` / `zoomOut` | Terminal font is the terminal's | — | **Out of scope** (a note in README: use the terminal's zoom; export `--font-size` covers images). |
| `open` file, `openSlide`, previews | Yes | `:link path:A-B` switches the code | **t-105** (`code.link`, each file with its own annotations); slides/markdown previews are out of scope (a slide deck tool already exists for that; Tiza is the code act). |
| Terminal actions (run command, wait for output) | Yes (editor terminal) | — | **Not adopted** (decided 2026-10-17; t-086 won't do). Stays with the editor's task / herdr pane. |
| `waitForTimeout`, `autoAdvanceAfter`, `loop`, timer | No | — | `wait` move (t-105), auto-advance per step and `loop` (t-099) as automation for recordings; the **timer is out of scope** (decided 2026-10-17). |
| `notes` per scene + presenter view | No | — | **Out of scope** (decided 2026-10-17): no presenter notes, no presenter view. |
| HTTP API `/api/next`, `/runById`, `/demos`, `/screenshot` | No | Unix socket: `step.next`, `step.set`, `scene.get`, `render.text`; `tiza render` → PNG | **Exists** (socket instead of HTTP). `/screenshot` ≈ `export_image`. A `step.goto {id}` with named steps would match `runById`. |
| Remote control PWA / clicker | No | Any client of the socket; keyboard → | **Out of scope** (decided 2026-10-17): no clicker feature; the socket and `tiza next|prev|goto` exist for automation (editor tasks, recordings). |
| MCP server | No | `tiza mcp` | **Exists** (ours is the whole surface, theirs is new). |
| Recording / export as video | Bundled ffmpeg records a second VS Code window | `render --out demo.gif` native (t-120), `.mp4`/`.webm` through the user's own ffmpeg, PNG/SVG per step, `.cast`/`.tape` (t-051) | **t-120** done: Tiza draws every frame itself (same pixels as the PNG export, one geometry per take), so the GIF needs nothing installed and MP4 only an ffmpeg on PATH — never bundled, not even behind a feature (Luis, 2026-10-18). t-101 (`--all-steps` stills) still open; t-089 was a duplicate (won't do). |
| Snippets gallery (parametrized step lists) | No | document `styles` (marks only) | **Not adopted** (decided 2026-10-17; t-102 won't do): boxes, arrows and notes are already one command each. |
| Slides, themes, layouts, Mermaid, PowerPoint/Keynote | Yes (webview) | — | **Out of scope.** Tiza is the code segment of a talk; slides live elsewhere. |
| Copilot chat actions, EngageTime polls, QR | Yes / SaaS | — | **Out of scope.** |
| macOS desktop actions (focus mode, dock, caffeine) | OS | — | **Out of scope** (a tip in the recording profile docs at most). |
| Preflight check (validate the demo before presenting) | No | `tiza render` fails on bad anchors | **t-101** (`tiza check FILE`). |
| Redaction mode | Editor | — | Out of scope. |

## Proposal: "moves" for Tiza (editor-agnostic)

Tiza already has the *static* half of Demo Time (steps as visible states). The *dynamic* half — the code itself changing while you talk — is what a presenter misses most, and it does not need an editor: Tiza owns the buffer it shows. The proposal, for ADR 0007 (t-084):

- Keep annotations + `step` as they are. Add an optional `moves` list to the document: each move has `at_step` and an action from a *small* set that makes sense in a terminal:
  - `code.patch {insert|replace|delete, position|find, content, typing: instant|line|char}` — the code evolves; annotations re-anchor by `find`, those whose text disappears are hidden.
  - `code.link {file, lines}` — switch to another file/range (the terminal's "open").
  - `cursor {line, col?}` / `select {from, to}` — direct attention like a presenter's mouse.
  - `wait {ms}` and per-step `auto_advance` — unattended demos and loops.
  - ~~`notes` per step~~ — dropped 2026-10-17 (no speaker features).
- Named steps (`steps: [{id, title}]`) so the socket gets `step.goto {id}` and the status line shows a title.
- Everything that needs the editor (open files there, run terminal commands) stays with the editor's task runner; docs/zed.md and the herdr guide show how to drive both from one keystroke.

This keeps the `.tiza` file as the single source for a demo, usable from Zed, Helix, Neovim, or a bare terminal, with the socket as the remote control.

## Candidate tasks (on the board under the Demo Time epic, as of 2026-10-17)

- t-084 ADR 0007: moves and named steps (this proposal, decide the schema). Accepted.
- t-097 Range anchors by two markers (`find_start` / `find_end`), the terminal version of placeholders. Done.
- t-098 `code.patch` moves with typing modes and deterministic re-anchoring.
- t-105 The other moves: `cursor`, `select`, `wait`, `code.link` (each file with its own annotations).
- t-099 Named steps + `step.goto`, auto-advance, loop (automation only; no timer).
- t-101 `tiza check` preflight; `render --all-steps` for one image per step and the ffmpeg recipe.
- t-106 Follow mode while moves are applied (milestone `later`).
- Won't do: t-100 presenter notes, t-102 annotation templates, t-085 (superseded by t-098), t-086 terminal moves, t-089 preflight/recording (duplicate of t-101 + t-051).
