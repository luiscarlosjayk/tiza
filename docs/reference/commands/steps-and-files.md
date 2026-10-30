# Step and file commands

Use these commands to go to a step, to show another file, and to follow the editor. A frame shows one step, so this page has no frames.

## `:step`

Goes to a step.

```
:step N
```

- `:step N` can go to the step after the last step. That step is empty. New annotations go there. On that step, `:step N` again and `step.set` stay on it. `n` stays on it too, unless the document loops: then `n` goes to step 0.
- In presentation mode, `:step N` to the step on screen does what `:goto N` does there: the view goes back to the target of the `scroll` of the step, and what comes in at the step comes into the view. A `cursor` or a `code.link` move of the step puts the cursor and the view back on its line. See [Present](../keys.md#present).
- When a move does not apply, the status bar shows the warning of the move in place of `step N/M`.
- `:step ID N` moves an annotation to a step. See [`:step ID N`](change.md#step-id-n).

## `:next` and `:prev`

Go to the next step or to the previous step.

```
:next
:prev
```

- The keys `n` and `p` do the same. See [Keys](../keys.md).
- When a move does not apply, the status bar shows the warning of the move in place of `step N/M`.

## `:goto`

Goes to a named step or to a step number. Tiza applies the steps from step 0 up to that step.

```
:goto ID
:goto N
```

For example, `:goto borrow` goes to the step with the name `borrow`. Named steps are in [Go to a named step](../document-format.md#go-to-a-named-step).

- `:goto` to the step on screen applies the moves of that step to the view again. A `scroll` move puts the view back on its target. A `cursor` or a `code.link` move puts the cursor and the view back on its line. When the live pointer stays, the cursor stays on the pointer.
- In presentation mode, what comes in at that step also comes into the view again.

## `:link`

At step 1 or later, shows another file from this step on.

```
:link PATH[:A-B]
:link
```

For example, `:link src/lib.rs:10-30` shows lines 10 to 30 of `src/lib.rs` from the step on screen. Tiza adds a `code.link` move.

- At step 0, `:link PATH` does the same as `:ref PATH`. See [The document and its code](../document-format.md#the-document-and-its-code).
- A range can be `A-B`, `A..B` or `A`.
- `:link` without a path makes the document read its code from the source file again.
- At step 1 or later, `:link` refuses a range that starts at 0, or that has `A` after `B`.
- At step 1 or later, `:link` also refuses digits, `-` and `.` that are not `A`, `A-B` or `A..B`.
- `:link` does not put the lines of a range in order.

## `:scroll`

At step 1 or later, adds a `scroll` move at the step on screen. The view moves to the target from that step on.

```
:scroll N [align=A]
:scroll @'TEXT' [occurrence=N] [align=A]
:scroll ID [align=A]
:scroll annotation=ID [align=A]
```

For example, `:scroll 120 align=top` puts line 120 at the top of the view at the step on screen.

- `N` or `LN` (`50` or `L50`, as `move.list` writes it) is a line. `@'TEXT'` is a text: the view goes to its first match, or to the match that `occurrence=N` names. Another word is the `id` of an annotation. For an `id` that has the form of a line, for example `12` or `L5`, write `annotation=ID`.
- `align=A` is `top`, `center`, `bottom` or `show`. Without it, the target goes to the center.
- The view moves at once. See [Scroll](../document-format.md#scroll).
- At step 0, `:scroll` gives an error, because step 0 is the code before any move.
- `:scroll` refuses a target that is not in the code at the step on screen, and adds no move. For example: `scroll: line 120 is not in the code shown at this step (lines 1-80)`.

## `:ref`

Makes the document read its code from a file.

```
:ref PATH[:A-B]
:ref
```

- `:ref` without a path makes the document read its code from the source file again.
- `:ref` and `:link` read a range as `tiza FILE:A-B` does. A range that is not correct gives an error, and the document does not change.
- The code of a document is in [The document and its code](../document-format.md#the-document-and-its-code).

## `:embed`

Copies the code into the document. The document then does not read a file. See [The document and its code](../document-format.md#the-document-and-its-code).

```
:embed
```

## `:follow`

Starts follow mode, stops it, or shows its state. In follow mode, Tiza reloads the code when the file changes on disk.

```
:follow on
:follow off
:follow
```

- `:follow on` also accepts `yes` and `start`. `:follow off` also accepts `no` and `stop`.
- Follow mode is in [Follow the editor](../../guides/follow-the-editor.md). The cases with no reload are in [Messages of follow mode](messages.md#messages-of-follow-mode).
