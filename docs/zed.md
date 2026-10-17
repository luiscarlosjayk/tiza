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

## Gotchas

- The task picker is `task: spawn` (option-shift-t on macOS); the command palette does not list tasks. Tasks that use `$ZED_SELECTED_TEXT` only appear while there is selected text.
- After editing `tasks.json`, spawn the task from the list, not from a history entry (the clock icon): Zed re-runs the old definition ([zed#37720](https://github.com/zed-industries/zed/issues/37720)). Renaming the task is the reliable workaround.
- `allow_concurrent_runs: false` does not stop a second spawn of the same task: you get two Tiza tabs. The second one shows `local API disabled: an instance is already listening on …/tiza/default.sock (use --session for another)` — fine for a human, but agents and the MCP server talk to the first one; close it or start the second with `--session NAME`.
- The selected text arrives through stdin, so its numbering starts at 1; the 40-line task keeps the file's real numbers (`tiza file.rs:A-B`), so you can say "look at line 42" and match the editor.
- `--follow` (or `:follow on` later) reloads the code every time you save in Zed (⌘S), keeping the annotations anchored to their text; the status line shows `⟳ following · last HH:MM:SSZ`. It works with ranges too (`file.rs:A-B --follow` keeps showing those lines). If a save drops an annotation (its text is gone) and you ⌘Z in Zed and save again, the text is back but the annotation is not: press `u` once in Tiza — the harmless second reload leaves no undo step, so `u` goes straight to the reload that dropped it (the message says which: `undone: reload #N`, or `undone: note a3` when the last thing before it was an annotation).
- In Zed's terminal, use `line_height: "standard"` in the terminal settings so the box-drawing characters fit without gaps. A comment with a URL stays Cmd+clickable there.
- Theme: `t` toggles dark/light so Tiza matches the Zed theme you are using in the video.
