# Tiza from Zed

Goal: share your screen with Zed, select code and send it to Tiza in the integrated terminal (or in Ghostty) with a shortcut.

## Zed tasks

In `~/.config/zed/tasks.json` (or the project's `.zed/tasks.json`). These four were run in Zed 1.24 on macOS as written; copy them verbatim — Zed parses every `$ZED_…` itself, so no shell expansion (`${ZED_FILE##*.}` and the like) is allowed anywhere in a task, or the whole tasks file is rejected as «unknown variables» and no Tiza task shows:

```json
[
  {
    "label": "Tiza: current file",
    "command": "tiza",
    "args": ["$ZED_FILE"],
    "use_new_terminal": true,
    "reveal": "always"
  },
  {
    "label": "Tiza: current file, live (reload on save)",
    "command": "tiza",
    "args": ["$ZED_FILE", "--follow"],
    "use_new_terminal": true,
    "reveal": "always"
  },
  {
    "label": "Tiza: selected text",
    "command": "sh",
    "args": ["-c", "printf '%s' \"$ZED_SELECTED_TEXT\" | tiza --lang \"$ZED_FILE\""],
    "use_new_terminal": true,
    "reveal": "always"
  },
  {
    "label": "Tiza: 40 lines from the cursor",
    "command": "sh",
    "args": ["-c", "tiza \"$ZED_FILE:$ZED_ROW-$((ZED_ROW + 39))\""],
    "use_new_terminal": true,
    "reveal": "always"
  }
]
```

`--lang` accepts a path: Tiza takes the language from the file name (`stack.rs` → Rust, `Makefile` → make), from a shebang or modeline in the text when the name says nothing, else plain text; `:lang sh` overrides it inside the TUI. A range past the end of the file is clamped with a notice, so the 40-line window is safe near the bottom.

Shortcut in `~/.config/zed/keymap.json`:

```json
[
  { "context": "Editor", "bindings": { "cmd-shift-d": ["task::Spawn", { "task_name": "Tiza: current file" }] } }
]
```

**Rust files need a Cargo workspace.** In a Rust buffer Zed resolves `$ZED_FILE` / `$ZED_ROW` through rust-analyzer, and when it fails to find a workspace («Failed to discover workspace») every task that uses them is hidden from the picker and the shortcut does nothing, with no error. Open a loose `.rs` file from a crate (a `Cargo.toml` next to it is enough); any other language works from anywhere.

## Driving the demo from Zed (t-099)

`tiza next`, `tiza prev` and `tiza goto ID` step the Tiza instance that is running (its session socket; add `--session NAME` for another), so a Zed task can advance the demo while Zed shows the code. These three were run in Zed 1.24.2 on macOS 27.0.1 on 2026-10-18, with Tiza in Zed's terminal dock (inside Herdr, which also ran there). `goto` takes a step id from your document, e.g. `count` in `examples/tally-tour.tiza`, whose steps are main, args, count, report and recap:

```json
[
  { "label": "Tiza: next step", "command": "tiza", "args": ["next"], "reveal": "never", "reveal_target": "center", "hide": "on_success" },
  { "label": "Tiza: previous step", "command": "tiza", "args": ["prev"], "reveal": "never", "reveal_target": "center", "hide": "on_success" },
  { "label": "Tiza: go to «count»", "command": "tiza", "args": ["goto", "count"], "reveal": "never", "reveal_target": "center", "hide": "on_success" }
]
```

Why `"reveal_target": "center"`: Zed runs every task in a terminal tab, even with `"reveal": "never"`. By default that tab opens in the terminal dock and closes again, so whatever lives in the dock blinks on every step — Tiza, or all of Herdr (its bar and panes) when Herdr runs there. With `"center"` the task tab opens among the editor's tabs and leaves the dock untouched; `"hide": "on_success"` closes it as soon as the step has changed. (With the dock target, `"hide": "never"` is worse: the task tab stays in front and covers the dock.) Tiza itself does not flicker: a step from a task draws the same frame as pressing `n`.

The leftover is the editor's tab bar, which shows the task tab for an instant. To record, capture only Tiza's area (the Herdr pane or the dock), or step without a task — `n`/`p` in Tiza, `auto_advance_ms` on the steps, or an agent with the MCP `goto_step` — which never opens a tab.

When a task fails it exits 1 and, since `hide` is `on_success`, its terminal stays open with the message: `no Tiza instance is listening on …` with nothing running, and `no step is named "nope" (named steps: main, args, count, report, recap)` for a `goto` to an id the document does not have. Bind them like any task (`"cmd-alt-right": ["task::Spawn", {"task_name": "Tiza: next step"}]`); pairing one with an editor action of your own is the ADR 0007 §6 idea.

## Gotchas

- The task picker is `task: spawn` (option-shift-t on macOS); the command palette does not list tasks. Tasks that use `$ZED_SELECTED_TEXT` only appear while there is selected text.
- After editing `tasks.json`, spawn the task from the list, not from a history entry (the clock icon): Zed re-runs the old definition ([zed#37720](https://github.com/zed-industries/zed/issues/37720)). Renaming the task is the reliable workaround.
- `allow_concurrent_runs: false` does not stop a second spawn of the same task: you get two Tiza tabs. The second one shows `local API disabled: an instance is already listening on …/tiza/default.sock (use --session for another)` — fine for a human, but agents and the MCP server talk to the first one; close it or start the second with `--session NAME`.
- The selected text arrives through stdin, so its numbering starts at 1; the 40-line task keeps the file's real numbers (`tiza file.rs:A-B`), so you can say "look at line 42" and match the editor.
- `--follow` (or `:follow on` later) reloads the code every time you save in Zed (⌘S), keeping the annotations anchored to their text; the status line shows `⟳ following · last HH:MM:SSZ`. It works with ranges too (`file.rs:A-B --follow` keeps showing those lines). A `.tiza` with moves opens at its own step (its last one when it names none): there follow is on and waits, and a save loads when you go back to step 0 (the bar says `following main.rs (waiting: moves applied at step 1; loads at step 0)`). If a save drops an annotation (its text is gone) and you ⌘Z in Zed and save again, the text is back but the annotation is not: press `u` once in Tiza — the harmless second reload leaves no undo step, so `u` goes straight to the reload that dropped it (the message says which: `undone: reload #N`, or `undone: note a3` when the last thing before it was an annotation).
- In Zed's terminal, use `line_height: "standard"` in the terminal settings so the box-drawing characters fit without gaps. A comment with a URL stays Cmd+clickable there.
- Theme: `t` toggles dark/light so Tiza matches the Zed theme you are using in the video.
