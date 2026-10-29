# Change the code between steps

Use this page to change the code at a step, show another file or name the steps of a document. The fields of each move are in [Moves](../reference/document-format.md#moves).

Tiza is a terminal tool that draws annotations on code. To install it, see [Install](../guides/install.md). To draw the annotations, see [Draw for a presenter](draw-for-a-presenter.md).

A presenter can start with a prompt like this one:

```text prompt
Use Tiza to show a fix in the code that I have open.
At step 1, mark the problem.
At step 2, type the fix with a move, and explain it in a note.
```

## Add a move

When the lesson adds or changes code between steps, add a move. Do not open a second document.

1. Call `add_move` with `at_step`, `op` (`insert`, `replace` or `delete`), one target and `content`. A `delete` takes no `content`.
2. To type the content on the screen, add `"typing": "line"` or `"typing": "char"`.
3. Add the annotations of the new code with the same `step` as the `at_step` of the move.
4. Call `check`.

For example, at step 2 of `examples/ownership.rs`, this move types a new line 8. The note is on that new line:

```json
{"name":"add_move","arguments":{"at_step":2,"op":"insert","after":"let n = len(&t);","content":"    let first = t.chars().next();","typing":"line"}}
{"name":"add_note","arguments":{"line":8,"text":"a second shared borrow","step":2}}
```

- Tiza checks an anchor on the code of the step of the annotation, at any step on the screen.
- `n` applies the move, and `p` reverts it. `tiza render --step N` and the exports apply it at once.
- `get_code` gives the code of the step on the screen. To read the code of another step, go to that step first. This changes the step on the screen of the presenter. Ask the presenter before you do this.
- `list_moves` gives each move with its index, and the ids of the annotations that are unanchored at the step on the screen.

## Targets

- Give a target that matches exactly once. When the text occurs more than once, give `occurrence`. See [Where a `code.patch` move goes](../reference/document-format.md#where-a-codepatch-move-goes).
- An `occurrence` after the last match is an error that names the move. A target that matches more than once without `occurrence` is an error too.

## Annotations on changed lines

A move takes the annotations of earlier steps with their lines. The rules are in [Annotations after a `code.patch` move](../reference/document-format.md#annotations-after-a-codepatch-move).

- A `replace` can take away the text under a highlight, a mark or an arrow end. Then Tiza hides the annotation as `(unanchored)`. To keep it, give its new place in `rebind`.
- Tiza finds the place of a `rebind` entry on the code after the move. It checks the entry when you add the move.
- Tiza refuses a `rebind` entry for an id that is not in the document.

An entry can lose its annotation later: a delete, a paste or a reload. Then Tiza keeps the entry and ignores it.

- `check` gives `rebind names ID, which is not in the document; the entry is ignored`.
- The entry works again when an annotation with that id comes back, with `u` or with your own `id`.
- Tiza refuses an annotation with that id when the entry does not work with it. Tiza never gives that id to a new annotation itself.
- A `scene.put` keeps an ignored entry only on the same move with the same code. Otherwise Tiza refuses the put with `move #N, step S: rebind names ID, which was dropped earlier: the entry is ignored and stays only on the same move with the same code; remove it from the rebind to change this move or the code`.

## Add a move to a document with annotations

A new move keeps the annotations and the later moves in place. Tiza carries them through the new move.

- Tiza carries the annotations of the file of the move, at its step or later.
- Tiza carries the line numbers of the moves that run after it: `position`, a `cursor`, a `select` by line and a `code.link` window.
- For example, insert two lines above line 8 at step 1. A note of step 2 on line 8 goes to line 10.
- `delete_move` carries them back. Thus a delete gives back the document from before the move.
- There is no tool to change a move. To change a move, delete it and add it again.

Tiza refuses a change that a delete cannot undo. The error tells what the change loses and what to do. For example:

```
annotation a2, step 1: move #3 (step 1), the one being added, removes line 17, where its end is; delete or move a2 first
```

Tiza refuses a move that does one of these:

- It removes the line of an annotation or of an arrow end, or the first or the last line of a range.
- It changes the text under a mark or an arrow end. On a changed line, an anchor stays when the line is the same up to it.
- It removes the line after which a later insert goes, or an end of the range or the window of a later move.
- It moves the line of a `rebind` entry.
- It makes the text target of a later move match another line with the same text.

Tiza refuses `delete_move` in the same way in these cases:

- An annotation is on a line that the move wrote.
- A later move stops to apply. For example, a deleted `code.link` sends the later moves to another file.
- The move is the only `code.link` to a file with annotations. Delete those annotations first.

## Move the view

When the lesson talks about code that is far from the view, add a `scroll` move at that step. The view moves before the annotations of the step come in, so their effects play in view. A step with only a `scroll` move is a camera step.

```json
{"name":"add_move","arguments":{"at_step":4,"action":"scroll","annotation":"b2","align":"top"}}
{"name":"add_move","arguments":{"at_step":5,"action":"scroll","find":"fn drop"}}
```

- The target is one of `line`, `find` and `annotation`. `align` is `top`, `center` (the default), `bottom` or `show`.
- A `find` goes to the first match. Give `occurrence` to choose another match.
- The annotation can be of a later step: the view moves to where it will come in.
- `add_move` refuses a target that does not resolve at its step. `check` gives a warning when a later change makes a target not resolve.
- The exports ignore `scroll` moves. See [Scroll](../reference/document-format.md#scroll).

## Show another file

A `code.link` move shows another file from its step, until a later link shows another file. A link to the main file of the document goes back to it. See [Several files](../reference/document-format.md#several-files).

```json
{"name":"add_move","arguments":{"at_step":2,"action":"code.link","file":"src/queue.rs","lines":"10-36"}}
```

- `get_code` gives the file on the screen, in its window. When a `code.link` shows the file, the first line names it, for example `# src/queue.rs:10-36`.
- `status` gives the file on the screen. See [Fields of `status`](../reference/socket-api.md#fields-of-status).
- A move carries a window as it carries a line number. An insert at the first line of the window, at the step of the link, makes the window start at the new lines.
- When a move hides annotations that were on the screen at a step, its reply has a `warning` with their ids. `check` gives the same as `outside_window` or `not_shown`.
- The `warning`, the status bar and `check` name a linked file as its `code.link` names it, and the main file as `code.file` names it. With embedded code, they give the file name, or `the main file` when the document has no file name.

For example, the `warning` can be:

```
a3, a4: outside the window at step 3 (src/queue.rs:10-36): kept, not drawn there
```

A `cursor`, `select` or `wait` move puts the cursor, selects a span or waits before the annotations of the step. See [Cursor, select and wait](../reference/document-format.md#cursor-select-and-wait).

## Name the steps

Named steps, auto-advance and loop are for automation: a recording, a screen that loops, an editor task. They add no speaker notes and no timer. The fields are in [Named steps](../reference/document-format.md#named-steps).

- No MCP tool sets them. Send `scene.patch` with `steps` and `loop` on the socket, or `tiza scene patch`. `scene.put` also sets them.
- `steps` in `scene.patch` replaces the whole list. `[]` clears it. `scene.get` gives the list.
- `goto_step` with `{"id": "borrow"}` goes to that step as `next_step` does from step 0. It also takes a step number.
- `status` gives the named step on the screen. See [Fields of `status`](../reference/socket-api.md#fields-of-status). `list_annotations` starts with the step label.
- Scripts and editor tasks use `tiza next`, `tiza prev` and `tiza goto ID`. See [Go to a named step](../reference/document-format.md#go-to-a-named-step).
- For the limits of the steps and the times, see [Rules the schema does not check](../reference/document-format.md#rules-the-schema-does-not-check).

## When the presenter edits the file

A reload or a paste changes the code of step 0. See [Special cases](../reference/document-format.md#special-cases).

Tiza did not write the change of a reload or a paste. Thus a move goes with the change only when the new place of its lines is certain.

- The rule: when the place of a line is not certain, the move is stale.
- The place is certain when the line is in the same start or the same end of the old and the new code, and the change is one block.
- A line next to the change whose text the change repeats has no certain place. A line inside a one-block change has a certain place only when the block kept its size.
- After several edits, each move on the lines from the change to the end of the code is stale, even when the text looks the same.
- A text target is stale when it matches nothing, or matches more than once.
- A move whose target a stale move wrote is stale too.

Tiza skips a stale move on `n` and `p`. The steps keep their numbers. The file keeps the move as you wrote it, but its line numbers follow a change with a certain shift.

- Tiza hides an annotation as `(unanchored)` when it is on the lines that a stale move writes.
- `list_moves` gives `stale: true` and `stale_reason`. `list_annotations` lists the stale moves last. `status` gives `stale_moves`, and `check` gives a `stale` error.
- A later change that makes the move match again clears the mark, and so does `u`. Code put back as it was gives back the move.
- A `scene.put` of the copy that `scene.get` gave keeps the same moves stale.
- `add_move`, `delete_move` and `scene.put` check again only the text targets. A stale move by line numbers stays stale until a change gives the place of its lines.
- `scene.get` gives the marks next to the document, in `stale` and `held`. Thus `export_image` skips the same moves as the screen.
- To fix a stale move, delete it with `delete_move`. Then add it again on the code as it is now.
- While a move is stale, `:q` refuses. See [Quitting](../concepts/saving.md#quitting).

The status bar of the presenter shows these messages:

| Message | When |
|---|---|
| `stale move skipped (u undoes or re-add): #N (WHY)` | A move became stale, or the reason of its mark changed. |
| `N stale moves skipped (u undoes or re-add): LIST` | The same, with more than one stale move. |
| `move #N matches the code again` | A change cleared the mark of a move. |
| `moves LIST match the code again` | A change cleared the marks of more than one move. |

- The message lists all the stale moves, not only the new one.
- One change can give both parts. Then ` · ` joins them, and the stale moves come first.

> **Warning:** After the instance closes, nothing marks a move. At the next open, a move by line numbers applies on the line that its number gives, with no message. A move whose text target still matches nothing stops the document from opening. Run `check` before the instance closes.

In a document that reads its code from a file, use `find`, not `position`. Then a change of the lines gives an error, not a move in the wrong place.
