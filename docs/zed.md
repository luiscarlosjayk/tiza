# Tiza from Zed

Use this page to send code from Zed to Tiza with one key, and to step a demo from Zed. Zed runs Tiza in its terminal through Zed tasks.

Tiza is a terminal tool that draws annotations on code. To install it, see [Install](guides/install.md).

## Add the tasks

1. Open `~/.config/zed/tasks.json` for all your projects, or `.zed/tasks.json` for one project.
2. Copy these tasks into the file as they are. They were tested in Zed 1.25.3 on macOS, 2026-10-24.

```json
[
  { "label": "Tiza: current file", "command": "tiza", "args": ["$ZED_FILE"], "use_new_terminal": true, "reveal": "always" },
  { "label": "Tiza: current file, live (reload on save)", "command": "tiza", "args": ["$ZED_FILE", "--follow"], "use_new_terminal": true, "reveal": "always" },
  { "label": "Tiza: selected text", "command": "printf '%s' \"$SEL\" | tiza --lang \"$ZED_FILE\"", "args": [], "env": { "SEL": "$ZED_SELECTED_TEXT" }, "use_new_terminal": true, "reveal": "always" },
  { "label": "Tiza: 40 lines from the cursor", "command": "tiza", "args": ["$ZED_FILE:$ZED_ROW-$((ZED_ROW + 39))"], "use_new_terminal": true, "reveal": "always" }
]
```

3. To give a task a key, add a binding to `~/.config/zed/keymap.json`. For example:

   ```json
   [
     { "context": "Editor", "bindings": { "cmd-shift-d": ["task::Spawn", { "task_name": "Tiza: current file" }] } }
   ]
   ```

4. In Zed, run the action `task: spawn` (Option-Shift-T on macOS) to open the task list. The command palette does not list the tasks themselves.
5. Select a task. Tiza opens in a new tab of the Zed terminal.

| Task | What Tiza shows |
|---|---|
| `Tiza: current file` | The file that is open in Zed. |
| `Tiza: current file, live (reload on save)` | The same file in follow mode, which reloads the code each time that you save in Zed. |
| `Tiza: selected text` | The selected text. |
| `Tiza: 40 lines from the cursor` | 40 lines that start at the cursor line, with the line numbers of the file. |

- Zed shows `Tiza: selected text` only while text is selected.
- The selected text comes to Tiza through standard input. Thus its line numbers start at 1.
- With the 40-line task, Tiza shows the real line numbers of the file. Thus "line 42" in Tiza is line 42 in Zed.
- Near the end of the file, Tiza cuts the range at the last line and shows a message. See [Messages](#messages).

## Write your own tasks

- Zed replaces each `$ZED_…` variable itself. Do not use a shell expansion such as `${ZED_FILE##*.}` in a task. Zed then refuses the whole tasks file with the error "unknown variables", and shows no Tiza task.
- Zed runs each task with your shell, as `$SHELL -i -c`. The command line is `command`, then `args`. Thus your shell runs the pipe of the selected-text task and the arithmetic of the 40-line task.
- Do not set `"command": "sh"` with `-c` in `args`. Zed 1.25.3 then runs `sh` with no script. Tiza stops with `tiza: empty stdin` or `tiza: give a file …`.
- Zed shows a task that uses `$ZED_SELECTED_TEXT` only while text is selected.
- The selected-text task puts the selection in the variable `SEL`. Thus the shell does not read the quotes or the `$` signs in your code.

## Step the demo from Zed

`tiza next`, `tiza prev` and `tiza goto ID` change the step of the Tiza instance that runs. They find it through its session socket. Thus Tiza can run in the Zed terminal or in another terminal, such as Ghostty. Both terminals must have the same `$XDG_RUNTIME_DIR` or `$TMPDIR`. When Tiza runs with `--session NAME`, add `--session NAME` to the task too. `$TIZA_SOCKET` and `$TIZA_SESSION` also choose the instance.

`ID` is the id of a named step or a step number. For example, `examples/tally-tour.tiza` names its steps `main`, `args`, `count`, `report` and `recap`. These tasks change the step of that document:

```json
[
  { "label": "Tiza: next step", "command": "tiza", "args": ["next"], "reveal": "never", "reveal_target": "center", "hide": "on_success" },
  { "label": "Tiza: previous step", "command": "tiza", "args": ["prev"], "reveal": "never", "reveal_target": "center", "hide": "on_success" },
  { "label": "Tiza: go to «count»", "command": "tiza", "args": ["goto", "count"], "reveal": "never", "reveal_target": "center", "hide": "on_success" }
]
```

Give each task a key as in [Add the tasks](#add-the-tasks), for example `"cmd-alt-right": ["task::Spawn", {"task_name": "Tiza: next step"}]`. These three tasks were tested in Zed 1.24.2 on macOS 27.0.1 on 2026-10-18. Tiza ran in the terminal dock of Zed, inside Herdr, which also ran in the dock.

The options `reveal`, `reveal_target` and `hide` keep the dock still:

- Zed runs every task in a terminal tab, even with `"reveal": "never"`. By default, that tab opens in the terminal dock and then closes. Thus the content of the dock blinks at each step: Tiza, or all of Herdr when Herdr runs there.
- With `"reveal_target": "center"`, the task tab opens among the editor tabs. The dock does not change.
- `"hide": "on_success"` closes the task tab when the step changes. With the default target, the dock, `"hide": "never"` leaves the task tab in front of the dock.
- Tiza does not blink. `tiza next` shows the same screen as `n`. `tiza prev` shows the same screen as `p`.
- The tab bar of the editor shows the task tab for a short time. To record without it, capture only the area of Tiza: the Herdr pane or the dock.
- You can also step without a task. Press `n` and `p` in Tiza, give the steps an `auto_advance_ms`, or let an agent call the MCP tool `goto_step`. These ways open no tab.
- In [supervised mode](guides/supervise-an-agent.md), Tiza holds `tiza next`, `tiza prev` and `tiza goto` for your review. Then a key in Zed does not change the step at once.

Tiza does not run an action of the editor at a step. You can put a Tiza command and an editor action together in your own task.

When a step command succeeds, it prints the step that it reached, for example `step 3/5 · count.rs: one pass`. When it fails, it exits with code 1. Because `hide` is `on_success`, the task terminal stays open and shows the message. See [Messages](#messages).

## Messages

| Message | When |
|---|---|
| `the file has N lines: range A-B clamped to C-D` | The range of the 40-line task goes past the end of the file. |
| `opened the existing PATH` | The lines of the 40-line task have a `FILE.A-B.tiza` document from an earlier session, and Tiza opened it. |
| `local API disabled: an instance is already listening on PATH (use --session for another one)` | A second Tiza starts in a session that already has an instance. |
| `tiza: empty stdin` | The standard input of Tiza is empty or has only blank characters. |
| `tiza: give a file (tiza src/main.rs:12-48) or pipe code through stdin (pbpaste \| tiza --lang rs)` | Tiza gets no file, and its standard input is a terminal. |
| `` tiza: no Tiza instance is listening on PATH (open `tiza <file>` or pass --socket/--session) `` | A step command finds no instance in the session. |
| `tiza: no step is named "ID" (named steps: IDS)` | `tiza goto` gets an id that the document does not have. |
| `tiza: no step is named "ID": the document has no named steps` | `tiza goto` gets an id, and the document names no step. |
| `tiza: step N is beyond the last step (M)` | `tiza goto` gets a number after the last step. |

Tiza shows the first two messages in the status bar. It writes the others on stderr. For a file with one line, the first message says `line`, not `lines`. The message `no Tiza instance is listening on …` ends with the error of the system. The messages of `tiza goto` end with an error code in parentheses. For example:

```
tiza: no step is named "nope" (named steps: main, args, count, report, recap) (not_found)
```

The other messages of `tiza` are in [Messages](reference/cli.md#messages).

## Special cases

### Rust files and Cargo

In a Rust file, Zed gets `$ZED_FILE` and `$ZED_ROW` through rust-analyzer. When rust-analyzer finds no workspace, it shows "Failed to discover workspace". Zed then hides each task that uses these variables, and the key does nothing. Zed shows no error.

Open the Rust file in a folder that has a crate. A `Cargo.toml` next to the file is enough. Files in other languages work from any folder.

### Run a task again after an edit

After you edit `tasks.json`, select the task from the list. Do not select it from the history (the clock icon). Zed runs the old definition from the history ([zed#37720](https://github.com/zed-industries/zed/issues/37720)). To avoid the problem, give the task a new name.

### Two runs of the same task

In Zed 1.25.3, `"allow_concurrent_runs": false` did not stop a second run of the same task. You get two Tiza tabs. The second Tiza shows `local API disabled: an instance is already listening on PATH (use --session for another one)`. Agents and the MCP server talk to the first Tiza. Both instances autosave the same `FILE.tiza` at each change, so each one writes over the other. Close the second one, or start it with `--session NAME`.

### Where Tiza saves

Autosave writes the document when you make your first change. The task decides the name of the document:

| Task | Document |
|---|---|
| `Tiza: current file` and the follow-mode task | `FILE.tiza`, next to the file |
| `Tiza: 40 lines from the cursor` | `FILE.A-B.tiza`, next to the file, for the lines `A-B` on screen |
| `Tiza: selected text` | `tiza-TIME.tiza`, in the folder where the task runs |

`TIME` is the time in seconds since 1970. Zed runs a task in the root of the project, unless the task sets `cwd`. When the selected text starts with `{` and contains `"schema"`, Tiza reads it as a `.tiza` document. Then it saves to `tiza-stdin.tiza`. See [Saving](concepts/saving.md).

A range never shows the annotations of `FILE.tiza`.

When you run the 40-line task again on the same lines, Tiza opens `FILE.A-B.tiza` with its annotations.

### The language of the selected text

When the value of `--lang` contains a `/` or a `\`, Tiza reads it as a path. `$ZED_FILE` is an absolute path, so the selected-text task always gives one. Tiza then finds the language as it does for a file: from an extension that has a grammar, or a well-known name such as `Makefile`. Next it looks for a `#!` line on the first line of the selected text, or a modeline in its first or last five lines. Otherwise it shows plain text. See [Command line](reference/cli.md).

To change the language in Tiza, type `:lang LANG`, for example `:lang sh`. `:lang auto` finds the language from the selected text only.

### Follow mode with Zed

The follow-mode task, `--follow` and `:follow on` reload the code each time that you save in Zed (Cmd-S). Follow mode keeps each annotation on its text when the code changes. Follow mode also works with a range: `tiza FILE:A-B --follow` keeps the line numbers `A-B` on screen. [Follow the editor](guides/follow-the-editor.md) gives the status bar states and the messages. It also explains how a save waits while a move changes the code at the step on screen. To restore an annotation that a save dropped, see [Undo a reload](guides/follow-the-editor.md#undo-a-reload).

### The Zed terminal

- Set `"line_height": "standard"` in the `terminal` settings of Zed. Then the box-drawing characters join with no gaps.
- In the Zed terminal, Cmd-click on a URL in a comment opens it. This also works while mouse capture is on.
- Press `t` to change between the dark and the light theme. Use the theme that matches your Zed theme in a video. See [Themes](reference/themes.md).

## See also

- [Follow the editor](guides/follow-the-editor.md)
- [Use the mouse](guides/present.md)
- [Terminal support](reference/terminals.md)
- [Keys](reference/keys.md)
- [Check before you record](guides/check-before-you-record.md)
