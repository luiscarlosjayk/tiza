# Step and file commands

Use these commands to go to a step, to show another file, and to follow the editor. A frame shows one step, so this page has no frames.

## `:step`

Goes to a step.

```
:step N
```

- `:step N` can go to the step after the last step. That step is empty. New annotations go there.
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
```

For example, `:goto borrow` goes to the step with the name `borrow`. Named steps are in [Go to a named step](../document-format.md#go-to-a-named-step).

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

## `:ref`

Makes the document read its code from a file.

```
:ref PATH[:A-B]
:ref
```

- `:ref` without a path makes the document read its code from the source file again.
- With `:ref`, and with `:link` at step 0, a range that is not correct becomes part of the file name. Then Tiza cannot read the file.
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
