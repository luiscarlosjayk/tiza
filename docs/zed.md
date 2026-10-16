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
    "label": "CodeDraw: selection",
    "command": "sh",
    "args": ["-c", "printf '%s' \"$ZED_SELECTED_TEXT\" | codedraw --lang \"${ZED_FILE##*.}\""],
    "use_new_terminal": true,
    "reveal": "always"
  },
  {
    "label": "CodeDraw: from the current line",
    "command": "codedraw",
    "args": ["$ZED_FILE:$ZED_ROW"],
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
- `codedraw file.rs:A-B` keeps the real line numbering, so you can say "look at line 42" and match the editor.
- In Zed's terminal, use `line_height: "standard"` in the terminal settings so the box-drawing characters fit without gaps.
- Theme: `t` toggles dark/light so CodeDraw matches the Zed theme you are using in the video.
