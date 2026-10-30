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
| `⟳ paused` | Follow mode is on, and a `code.patch` is applied at this step. A save waits. See [Moves](#moves). |
| `⟳ paused · changed` | Follow mode is paused, and a save waits to load. |
| `⟳ paused (file removed)` | Follow mode is paused, and the file is not on disk. |

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

The messages of follow mode are in [Messages of follow mode](../reference/commands/messages.md#messages-of-follow-mode).

## Special cases

### Moves

Only a `code.patch` move changes the text of the code. A `cursor`, `select`, `wait` or `code.link` move does not change the text. Thus a save at a step where no patch is applied loads at once, also when other moves are applied. The step keeps the file that its `code.link` shows, and step 0 shows the new code.

Follow mode is paused at a step where a `code.patch` is applied. A patch is applied when its `at_step` is that step or an earlier step. The patch can change any file of the document.

1. The bar shows `⟳ paused`.
2. A save in the editor does not load. It is a change that waits, and the bar shows `⟳ paused · changed`. Only the newest save counts.
3. The first change of a pause gives one message: `step N loads the change to FILE`. Step N is the last step before the first patch. The next saves of the same pause give no message.
4. While a change waits, the document keeps the code on screen as embedded code. If you quit, the document opens as you saw it.
5. At step N or an earlier step, Tiza loads the change at once. This is one reload. `u` undoes it.

- The moves apply again on the new code. A move that does not match the new code is stale: Tiza skips it and tells you one time. In a `loop`, this message comes one time for each change, not at each loop.
- A jump to step N or earlier, such as `:goto 0` or `step.goto`, loads the change before the reply.
- When you save the file back to the code on screen, no change waits. No new message comes, and the message about the change goes from the bar.
- When the file is removed during a pause, the bar shows `⟳ paused (file removed)`. When the file comes back with other code, that code is the change that waits.
- When the file is not valid UTF-8, the message `cannot follow FILE: not valid UTF-8 (keeping the previous content)` comes one time. That file is not a change. At step N, Tiza reads the file again.
- In presentation mode, no message comes. When you leave presentation mode, the bar shows the state.
- `:follow off` drops a change that waits. The message is `follow off: the change to FILE is not loaded`. When an agent turns follow mode off, the message about the change goes from the bar.
- A save while you type a `:` command, select text with `v` or `V`, have a panel open, or while a move types its code, waits until you finish.

A document opens at its saved `step`, or at step 1 when it has none. When a patch is applied at that step, `--follow` and `:follow on` start follow mode paused. The message is `following FILE; step N loads a save`. When the file has other code than the screen, that code is the change that waits.

Tiza follows only the main file. Tiza reads a file that a `code.link` shows when the document opens. A save of that file does not load.

### Link another file

- `:link PATH` or `:ref PATH` at step 0 to a file with other code loads the code of that file, as a reload does. The screen always shows what the saved document will show.
- When the file has the same code as the screen, Tiza only references the file. There is no reload and no undo step.
- When follow mode is on, follow mode moves to the new file. A save of the old file does not reload anything. `u` moves follow mode back.
- Tiza refuses a file that a `code.link` shows already, because its annotations belong to that view.
- `:link` with no path changes nothing when the document references its file already.

### An empty file

Some editors save a file with two writes. The first write is empty. When Tiza reads an empty file, it waits 120 ms more and reads the file again. A file that stays empty reloads as empty code. `u` brings the annotations back.

## Agents and follow mode

Agents control follow mode through the socket methods `follow.set` and `follow.status`, and the MCP tool `set_follow`. `follow.status` gives `enabled`, `path`, `reloads`, `reads`, `hash`, `last_reload`, `missing`, `paused`, `pending`, `resume_step` and `reason`.

- The reply of `follow.set` comes at once, for example `following FILE`. The file watcher starts on a thread of its own.
- A save while the watcher starts loads when the watcher runs.
- `paused` is `true` at a step where a `code.patch` is applied. Then `resume_step` gives step N, where a change that waits loads. `pending` is `true` when a change waits.
- `status` gives the same `paused`, `pending`, `resume_step`, `reads` and `hash` in its `follow` field while follow mode is on. An MCP agent reads them with the tool `status`.
- `reason` tells why a save does not load now, or why follow mode is off. `reason` is absent when no reason applies.
- `hash` is the sha256 of the file as Tiza last read it. To know that Tiza read your save, compare `hash` with the sha256 of the file that you saved. When they are the same, Tiza read your save. `reloads`, `pending` and the status bar tell what Tiza did with it.
- When you save the same bytes again, `hash` does not change. Then wait until `reads` increases.

  ```sh
  shasum -a 256 main.rs     # or: sha256sum main.rs
  tiza api follow.status
  ```

- `hash` is `null` when Tiza could not read the file, for example when the file is removed. A file that is not UTF-8 has a hash: Tiza read it, and the status bar shows the error.
- `reads` counts the reads of the file. It increases also when a save changes nothing, for example a save of the same bytes or of other line ends. When the watcher starts after `follow.set`, Tiza reads the file one more time.
- Saves that come close together can give one read. Wait until `reads` is more than before, not for one read for each save.
- While the presenter types a `:` command, selects text or has a panel open, or while a move types its code, a save waits. Tiza reads it when they finish.
- With follow mode off, `follow.status` has no `reads` and no `hash`. The counts start again at each `follow on`.
- `scene.get` gives `follows`. A `scene.put` that sends `follows` sets the file that `:follow on` reloads.
- A `scene.put` of the open document without `follows` keeps the file that the open document follows.
- While follow mode is paused, a `scene.put` of the code on screen works, also when a change waits. Thus `scene.get` and then `scene.put` of the same copy work. The code on screen does not change, and the change that waits still loads at `resume_step`. The other parts of the document, for example the annotations, change as you sent them.
- While a change waits, and while the followed file is missing or cannot be read, a `scene.put` cannot change `follows`: that is the file that follow reads next. Send `follows` as `scene.get` gives it, or leave it out. The same file written another way, for example `./main.rs` or its absolute path, is the same `follows`.
- When a `scene.put`, `scene.patch` or `move.delete` leaves no `code.patch` applied at the step on screen, the change that waits loads before the reply, as at a step change. When a change of the moves moves the resume step and the change still waits, the notice on the bar names the step where it loads now, if the notice is still there.
- While a change waits, a `scene.put` with other code fails, also with the code of the file on disk. The code on screen is not what the file holds now, and the pause stops new code at a step with a `code.patch`. The error gives the step where the file loads. To change the code, edit the file.

| `reason` | When |
|---|---|
| `paused; loads at step 0` | Follow mode is on, and a `code.patch` is applied at the step on screen. |
| `the code is pasted, not FILE; :follow on reloads FILE` | Follow mode is off after a paste. |
| `the code is the paste again, not FILE; :follow on reloads FILE` | Follow mode is off, and an undo or a redo brought back a paste. |
| `the code on screen is not FILE; :follow on reloads FILE` | Follow mode is off, and an undo or a redo brought back code that the file does not have now. |
| `cannot watch DIR: ERROR` | Follow mode is off, because the file watcher could not start. |

- When the first patch is at step 2 or later, the first row ends with `loads at step M or earlier`. `M` is the step before the first patch.
- When the file holds the code on screen again, follow mode is off and `reason` is absent.

## How it works

Tiza watches the folder of the file, not only the file. Some editors, such as Zed, save to a temporary file and then rename it. Tiza waits about 120 ms after the last write, then reads the file. A line diff matches the old code with the new code. Each annotation goes to the line that matches its old line.
