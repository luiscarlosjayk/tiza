# CodeDraw from Zed

Goal: share your screen with Zed, select code and send it to CodeDraw in the integrated terminal (or in Ghostty) with a shortcut.

## Zed task

In `~/.config/zed/tasks.json` (or the project's `.zed/tasks.json`):

```json
[
  {
    "label": "CodeDraw: current file",
    "command": "codedraw",
    "args": ["$ZED_FILE"],
    "use_new_terminal": true,
    "allow_concurrent_runs": false,
    "reveal": "always"
  },
  {
    "label": "CodeDraw: current file, live (reload on save)",
    "command": "codedraw",
    "args": ["$ZED_FILE", "--follow"],
    "use_new_terminal": true,
    "allow_concurrent_runs": false,
    "reveal": "always"
  },
  {
    "label": "CodeDraw: selection",
    "command": "sh",
    "args": ["-c", "printf '%s' \"$ZED_SELECTED_TEXT\" | codedraw --lang \"${ZED_FILE##*.}\""],
    "use_new_terminal": true,
    "reveal": "always"
  },
  {
    "label": "CodeDraw: from the current line (40 lines)",
    "command": "sh",
    "args": ["-c", "codedraw \"$ZED_FILE:$ZED_ROW-$((ZED_ROW + 39))\""],
    "use_new_terminal": true,
    "reveal": "always"
  }
]
```

Shortcut in `~/.config/zed/keymap.json`:

```json
[
  { "context": "Editor", "bindings": { "cmd-shift-d": ["task::Spawn", { "task_name": "CodeDraw: current file" }] } }
]
```

Notes:

- Tasks that use `$ZED_SELECTED_TEXT` only appear when there is selected text.
- `${ZED_FILE##*.}` is the file extension; for a file without one (a `Makefile`, a script with a shebang) it expands to the whole path and CodeDraw falls back to plain text — pass the language by hand instead (`--lang sh`). A range past the end of the file is clamped with a notice, so the 40-line window is safe near the bottom.
- `--follow` (or `:follow on` later) reloads the code every time you save in Zed (⌘S), keeping the annotations anchored to their text; the status line shows `⟳ following · last HH:MM:SSZ`. It works with ranges too (`file.rs:A-B --follow` keeps showing those lines).
- `codedraw file.rs:A-B` keeps the real line numbering, so you can say "look at line 42" and match the editor.
- In Zed's terminal, use `line_height: "standard"` in the terminal settings so the box-drawing characters fit without gaps.
- Theme: `t` toggles dark/light so CodeDraw matches the Zed theme you are using in the video.
