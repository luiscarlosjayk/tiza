# Follow the editor

Use follow mode to edit the code in your editor while Tiza shows it. Tiza reloads the code each time that you save the file. It keeps your annotations on their text.

Tiza is a terminal tool that draws annotations on code. To install it, see [Install](install.md).

## Start and stop

1. Open the file with `--follow`, for example `tiza src/main.rs --follow`. In Tiza, you can also type `:follow on`.
2. Edit the file in your editor and save it. Tiza reloads the code about 120 ms after the last write.
3. Read the state of follow mode in the status bar.
4. Type `:follow off` to stop.

| Status bar | Meaning |
|---|---|
| `⟳ following` | Follow mode is on, and no reload occurred yet. |
| `⟳ following · last HH:MM:SSZ` | Follow mode is on. This is the time of the last reload. |
| `⟳ following (file removed; waiting)` | The file is not on disk, and Tiza waits for it. |

| Command | What it does |
|---|---|
| `tiza FILE --follow` | Opens the file with follow mode on. |
| `:follow on` | Starts follow mode. |
| `:follow off` | Stops follow mode. |
| `:follow` | Shows the state: the number of reloads and the time of the last one. |
| `u` | Undoes the last reload, or the last change. |

To use Tiza next to Zed, see [Tiza from Zed](../zed.md).

## What happens to annotations

Tiza keeps each annotation on the text that it points at. The rule is the same as for a paste.

- Lines that stay the same keep their annotations, in order. A note on the second `}` stays on the second `}`.
- In a line, a span stays on the same occurrence of its text.
- A line that you rewrite in place keeps its note and the spans whose text is still there.
- When you rewrite several adjacent lines together, the lines pair one to one. An example is a parameter that you rename on the signature and on the line that uses it.
- A block that you move keeps its annotations when their line is unique and moves 50 lines or fewer. A box, callout or spotlight on the block moves with it.
- This rule works line by line. Tiza drops a note on a moved line that is not unique, such as `}`.
- When you edit or move one edge line of a box, callout or spotlight, it keeps its height from the edge that stays. It does not grow toward the line that moved.
- A box, callout or spotlight grows when you insert lines between its two edges.
- A formatter that splits or joins a line, such as rustfmt, moves the span to the line that now has its text.
- Tiza drops an annotation when its text is gone. It never moves the annotation to another copy of the text.
- When two identical adjacent lines become one line, the annotations of both stay on the line that is left.

### Undo a reload

- `u` undoes the whole reload. It brings back the annotations that the reload dropped.
- A reload that changes no annotation adds no undo step. Thus, after an undo in your editor, one `u` in Tiza brings back the dropped annotations.
- After `u`, the message names what it undid, for example `undone: reload #N`, `undone: note ID` or `undone: N annotations cleared`.
- When `u` brings back code that the file does not have now, follow mode turns off. See [When follow mode turns off](#when-follow-mode-turns-off).

## When follow mode turns off

These cases turn follow mode off. After each one, a save in the editor does not change the code on screen.

| Case | Message |
|---|---|
| You paste code. | `follow off: the code is pasted, not FILE; :follow on reloads FILE` |
| An undo or a redo brings back code that the file does not have now. | `follow off: the code on screen is not FILE; :follow on reloads FILE` |
| You type `:embed`. | `code frozen inside the .tiza; follow off: edits on disk no longer reach the screen (:link and :follow on to resume)` |
| You type `:follow off`. | `follow off` |

Follow mode also turns off in three more cases. The message gives the reason.

- An agent sends another document with `scene put`.
- The file watcher cannot start.
- `:link` or `:ref` names a file that Tiza cannot follow.

- Tiza keeps the pasted code. A save in the editor does not replace it.
- A paste of the exact content of the file keeps follow mode on. With follow mode off, that paste keeps the code reference to the file.
- When `u` brings back the code of the file, the message is `follow is off: :follow on reloads FILE`.
- When the code on screen is not the code of the file, the document keeps it as embedded code. This occurs after a paste of other code, or after an undo or a redo to code that the file does not have now.

## Turn follow mode on again

- `:follow on` loads the file over the code on screen. This is one reload. `u` undoes it.
- The document keeps the name of the file in `"follows": {"file": "main.rs"}`. Thus `:follow on` and `--follow` reload the file also after you quit and open the document again.
- `:embed`, `:link` and a reload back to the file remove `follows`.
- A document with embedded code and no `follows` has no file to follow. Type `:link PATH` first.

## Messages

| Message | When |
|---|---|
| `following FILE` | `:follow on` started follow mode. |
| `following FILE (waiting: moves applied at step N; loads at step 0)` | Follow mode started while a move is applied. See [Moves](#moves). |
| `already following FILE` | Follow mode was on already. |
| `FILE reloaded (#N): K annotations kept` | A reload kept all the annotations. |
| `FILE reloaded (#N): K annotations kept, D dropped (text gone; u undoes)` | A reload dropped `D` annotations. |
| `FILE changed on disk but moves are applied at step N: go back to step 0 to reload` | You saved the file while a move is applied. |
| `FILE removed; waiting for it to come back` | The file is not on disk. |
| `FILE is back; following it` | The file is on disk again. |
| `cannot follow FILE: not valid UTF-8 (keeping the previous content)` | The file is not text that Tiza can read. |
| `nothing to follow: the code is embedded (use :link path to reference a file)` | `:follow on` found embedded code. |
| `the .tiza already references FILE` | `:link` with no path found the code reference. |
| `the .tiza now references FILE` | `:link PATH` or `:ref PATH` found the same code in the file. |
| `FILE is shown by a code.link at step N, with annotations of its own: delete that link first to reference it` | `:link PATH` or `:ref PATH` named a file that a `code.link` shows. |
| `undone: reload #N` | `u` undid a reload. |
| `follow is off: :follow on reloads FILE` | `u` brought back the code of the file. |
| `follow off` | `:follow off` stopped follow mode. |
| `follow off: cannot watch DIR: ERROR` | The file watcher could not start. |

- When the first move is at step 2 or later, `loads at step 0` becomes `loads at step M or earlier`. `M` is the step before the first move.
- `already following FILE` gets the same `(waiting: …)` end while a move is applied.
- `DIR` is the folder of the file. `ERROR` is the error of the system, or `the watcher did not start`.

## Special cases

### Moves

A save in the editor waits while a move is applied at the step on screen. A move is applied when its `at_step` is that step or an earlier step.

1. The message is `FILE changed on disk but moves are applied at step N: go back to step 0 to reload`.
2. Until you go to a step with no move applied, the document keeps the code on screen as embedded code. If you quit, the document opens as you saw it.
3. At a step with no move applied, Tiza reloads the change at once. This is one reload. `u` undoes it.

When you save the file back to its old content, Tiza does not reload.

A document opens at its saved `step`, or at step 1 when it has none. When a move is applied at that step, `--follow` and `:follow on` start follow mode, and the reload waits. The message is `following FILE (waiting: moves applied at step N; loads at step 0)`.

### Link another file

- `:link PATH` or `:ref PATH` at step 0 to a file with other code loads the code of that file, as a reload does. The screen always shows what the saved document will show.
- When the file has the same code as the screen, Tiza only references the file. There is no reload and no undo step.
- When follow mode is on, follow mode moves to the new file. A save of the old file does not reload anything. `u` moves follow mode back.
- Tiza refuses a file that a `code.link` shows already, because its annotations belong to that view.
- `:link` with no path changes nothing when the document references its file already.

### An empty file

Some editors save a file with two writes. The first write is empty. When Tiza reads an empty file, it waits 120 ms more and reads the file again. A file that stays empty reloads as empty code. `u` brings the annotations back.

## Agents and follow mode

Agents control follow mode through the socket methods `follow.set` and `follow.status`, and the MCP tool `set_follow`. `follow.status` gives `enabled`, `path`, `reloads`, `last_reload`, `missing` and `reason`.

- The reply of `follow.set` comes at once, for example `following FILE`. The file watcher starts on a thread of its own.
- A save while the watcher starts loads when the watcher runs.
- `reason` tells why a save does not load now, or why follow mode is off. `reason` is absent when no reason applies.
- `scene.get` gives `follows`. A `scene.put` that sends `follows` sets the file that `:follow on` reloads.
- A `scene.put` of the open document without `follows` keeps the file that the open document follows.

| `reason` | When |
|---|---|
| `waiting: moves applied at step N; loads at step 0` | Follow mode is on, and a move is applied at the step on screen. |
| `the code is pasted, not FILE; :follow on reloads FILE` | Follow mode is off after a paste. |
| `the code is the paste again, not FILE; :follow on reloads FILE` | Follow mode is off, and an undo or a redo brought back a paste. |
| `the code on screen is not FILE; :follow on reloads FILE` | Follow mode is off, and an undo or a redo brought back code that the file does not have now. |
| `cannot watch DIR: ERROR` | Follow mode is off, because the file watcher could not start. |

- The first row can end with `loads at step M or earlier`, as in [Messages](#messages).
- When the file holds the code on screen again, follow mode is off and `reason` is absent.

## How it works

Tiza watches the folder of the file, not only the file. Some editors, such as Zed, save to a temporary file and then rename it. Tiza waits about 120 ms after the last write, then reads the file. A line diff matches the old code with the new code. Each annotation goes to the line that matches its old line.
