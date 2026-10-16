# Demo Time, mapped to CodeDraw (t-083)

Research note, 2026-10-16. Sources: demotime.show (getting started, JSON schema, actions/text, references/api, features/presenter-view). Demo Time is MIT-licensed; nothing of it is copied here, only its vocabulary where it helps.

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

Highlight specifics worth noting: whole-line highlight has a border; character-range highlight has none; `highlightBlur` / `highlightOpacity` dim everything else (the same idea as CodeDraw's spotlight); `zoom` scales the editor font for that step.

## What transfers to a terminal tool, and what does not

| Demo Time | Editor-bound? | CodeDraw today | Verdict |
|---|---|---|---|
| Scenes → moves, run in order with → | No | Steps (`step` per annotation, `n`/`p`) | **Exists.** Our step is a *state* (what is visible); theirs is an *action*. A CodeDraw "move" would be a state transition, see proposal. |
| `highlight` range, whole line / chars | No | `highlight`, `box`, marks | **Exists** (richer: labels, legends, colors). |
| `highlightBlur` / `highlightOpacity` | No | `spotlight` | **Exists.** |
| Placeholders (`// Start of demo1`) to locate ranges | No | `find` text anchors | **Exists** (find is per line; range-by-two-markers is a small add: t-097). |
| `insert` / `replace` with typing modes | No (needs a buffer we own) | `replace_code` (paste) only | **Planned**: code *diffs* as steps (show v1, then v2 with the added lines typed in). This is the biggest gap and the most valuable: "explain, then evolve the code". |
| `positionCursor`, `selection` | No | Cursor and selection exist for authoring, not as steps | **Partial**: a step that moves the cursor/selection is cheap once moves exist. |
| `zoomIn` / `zoomOut` | Terminal font is the terminal's | — | **Out of scope** (a note in README: use the terminal's zoom; export `--font-size` covers images). |
| `open` file, `openSlide`, previews | Yes | `:link path:A-B` switches the code | **Partial**: a move that links another file/range is the terminal equivalent; slides/markdown previews are out of scope (a slide deck tool already exists for that; CodeDraw is the code act). |
| Terminal actions (run command, wait for output) | Yes (editor terminal) | — | **Via the editor's task / herdr pane**, not CodeDraw. Document the pattern in docs/zed.md. |
| `waitForTimeout`, `autoAdvanceAfter`, `loop`, timer | No | — | **Planned** (small): auto-advance per step and a `timer` in the status line; `loop` for looping demos on a booth screen. |
| `notes` per scene + presenter view | No | — | **Planned**: presenter notes per step, shown on `?`-style overlay or sent to a second terminal/`scene.notes` over the socket (a second pane in herdr is the presenter view). |
| HTTP API `/api/next`, `/runById`, `/demos`, `/screenshot` | No | Unix socket: `step.next`, `step.set`, `scene.get`, `render.text`; `codedraw render` → PNG | **Exists** (socket instead of HTTP). `/screenshot` ≈ `export_image`. A `step.goto {id}` with named steps would match `runById`. |
| Remote control PWA / clicker | No | Any client of the socket; keyboard → | **Partial**: a `codedraw next` CLI shortcut and docs for Stream Deck (run a shell command) close it. |
| MCP server | No | `codedraw mcp` | **Exists** (ours is the whole surface, theirs is new). |
| Recording / export as video | No | PNG/SVG per step | **Planned** (small): `render --all-steps --out dir/` to get one image per step; a video is then a `ffmpeg` one-liner, documented. GIF via VHS (t-057). |
| Snippets gallery (parametrized step lists) | No | document `styles` (marks only) | **Later**: reusable annotation *templates* (e.g. "ownership move" = mark + arrow + note) with parameters. |
| Slides, themes, layouts, Mermaid, PowerPoint/Keynote | Yes (webview) | — | **Out of scope.** CodeDraw is the code segment of a talk; slides live elsewhere. |
| Copilot chat actions, EngageTime polls, QR | Yes / SaaS | — | **Out of scope.** |
| macOS desktop actions (focus mode, dock, caffeine) | OS | — | **Out of scope** (a tip in the recording profile docs at most). |
| Preflight check (validate the demo before presenting) | No | `codedraw render` fails on bad anchors | **Planned** (small): `codedraw check FILE` validates every anchor/step and reports missing `find` texts. |
| Redaction mode | Editor | — | Out of scope. |

## Proposal: "moves" for CodeDraw (editor-agnostic)

CodeDraw already has the *static* half of Demo Time (steps as visible states). The *dynamic* half — the code itself changing while you talk — is what a presenter misses most, and it does not need an editor: CodeDraw owns the buffer it shows. The proposal, for ADR 0007 (t-084):

- Keep annotations + `step` as they are. Add an optional `moves` list to the document: each move has `at_step` and an action from a *small* set that makes sense in a terminal:
  - `code.patch {insert|replace|delete, position|find, content, typing: instant|line|char}` — the code evolves; annotations re-anchor by `find`, those whose text disappears are hidden.
  - `code.link {file, lines}` — switch to another file/range (the terminal's "open").
  - `cursor {line, col?}` / `select {from, to}` — direct attention like a presenter's mouse.
  - `wait {ms}` and per-step `auto_advance` — unattended demos and loops.
  - `notes` per step — shown to the presenter, never to the audience (`scene.notes` over the socket for a second pane).
- Named steps (`steps: [{id, title}]`) so the socket gets `step.goto {id}` and the status line shows a title.
- Everything that needs the editor (open files there, run terminal commands) stays with the editor's task runner; docs/zed.md and the herdr guide show how to drive both from one keystroke.

This keeps the `.codedraw` file as the single source for a demo, usable from Zed, Helix, Neovim, or a bare terminal, with the socket as the remote control.

## Candidate tasks (added to the board under the Demo Time epic)

- t-084 ADR 0007: moves and named steps (this proposal, decide the schema).
- t-097 Range anchors by two markers (`find_start` / `find_end`), the terminal version of placeholders.
- t-098 `code.patch` moves with typing modes and re-anchoring.
- t-099 Named steps + `step.goto`, auto-advance, loop, timer in the status line.
- t-100 Presenter notes per step (`scene.notes`, overlay, second pane).
- t-101 `codedraw check` preflight; `render --all-steps` for one image per step and the ffmpeg recipe.
- t-102 Annotation templates with parameters (snippet gallery equivalent). Later.
