# MCP tools

This page lists the tools and the resources of the Tiza MCP server, with their arguments, their replies and their errors. To add the server to a client, see [Connect an agent](../agents/setup.md).

## The server

`tiza mcp` starts the MCP server on standard input and standard output. Your MCP client starts it for you.

- The server talks to the live instance through the local socket. Each tool sends one or more requests of the [Socket API](socket-api.md).
- `--socket PATH` and `--session NAME` select the instance, as for every command that uses the socket. See [Connect](socket-api.md#connect).
- The server starts when no instance runs. Each tool call then fails until an instance listens.
- Each request opens a new connection. Thus a call goes to the instance that listens at that time.
- `list_annotations` and `export_image` send two requests. The screen can change between the two.
- At the start, the server sends `instructions` to the client: what Tiza is, when to use it, and the order of the calls.

## Find the tools

A client can defer the tools. It then loads only the tool names and the `instructions` at the start. Claude Code does this with its tool search.

- These tools ask such a client to load them at the start: `status`, `get_code`, `render_text` and `add_box`. Their `_meta` has `"anthropic/alwaysLoad": true`.
- A client finds the other tools when it searches for what it wants to draw.
- A client that does not know the key ignores it.
- In Claude Code, the server entry can set `alwaysLoad` for all the tools of the server.
- With `"alwaysLoad": false`, the server entry defers all the tools of the server, these 4 tools included. This setting needs Claude Code v2.1.287 or later.

After you rebuild Tiza, the client keeps the old tool list. See [After you rebuild Tiza](../agents/setup.md#after-you-rebuild-tiza).

## Tools

The server has these tools. The third column names every socket method that each tool sends.

### Read

| Tool | What it does | Socket method |
|---|---|---|
| `status` | Gives the state of the instance and the `bridge` object of the server. | `status` |
| `get_code` | Gives the code on screen at the current step, with line numbers. | `code.get` |
| `list_annotations` | Lists the annotations, after a line with the current step. | `status`, `scene.list` |
| `check` | Gives the report of `tiza check --json` for the live document. | `check` |

### Annotate

| Tool | What it does | Socket method |
|---|---|---|
| `add_box` | Draws a box around a range of lines. | `annotation.add` |
| `add_arrow` | Draws an arrow from one point of the code to another. | `annotation.add` |
| `add_note` | Adds a short text on a line. | `annotation.add` |
| `add_callout` | Adds a block of text with a border that explains a range of lines. | `annotation.add` |
| `highlight` | Highlights a part of a line. | `annotation.add` |
| `add_mark` | Puts a mark of a kind on a part of a line. | `annotation.add` |
| `add_spotlight` | Dims the code and the annotations outside the ones that it names. | `annotation.add` |
| `define_style` | Defines a style, or changes it. | `style.set` |
| `list_kinds` | Lists the styles of the document, the personal styles of the user (`personal`), the built-in kinds and their aliases. A mark that uses a personal style copies it into the document. | `style.list` |

### Change annotations

| Tool | What it does | Socket method |
|---|---|---|
| `update_annotation` | Changes an annotation and keeps its id. | `annotation.update` |
| `delete_annotation` | Deletes an annotation. | `annotation.delete` |
| `clear_annotations` | Deletes all the annotations, or the annotations of one step. | `scene.clear` |

### Moves

| Tool | What it does | Socket method |
|---|---|---|
| `add_move` | Adds a move at a step. | `move.add` |
| `delete_move` | Deletes a move by its index. | `move.delete` |
| `list_moves` | Lists the moves, the unanchored annotations and the view on screen. | `move.list` |

### Steps

| Tool | What it does | Socket method |
|---|---|---|
| `set_step` | Shows a step. | `step.set` |
| `next_step` | Shows the next step. | `step.next` |
| `prev_step` | Shows the previous step. | `step.prev` |
| `goto_step` | Goes to a named step or to a step number, and applies every move up to that step. | `step.goto` |

### Focus and pointer

| Tool | What it does | Socket method |
|---|---|---|
| `set_focus` | Dims all the lines outside a range, or clears the focus. | `focus.set` |
| `point_at` | Puts the live pointer on a line or a range of lines. | `pointer.set` |
| `clear_pointer` | Turns the live pointer off. | `pointer.clear` |

### Theme and follow mode

| Tool | What it does | Socket method |
|---|---|---|
| `set_theme` | Changes the theme. | `theme.set` |
| `set_follow` | Turns follow mode on or off. | `follow.set` |

### Render and export

| Tool | What it does | Socket method |
|---|---|---|
| `render_text` | Gives the take at the current step as plain text. | `render.text` |
| `export_image` | Writes an export of the live document to a file. | `scene.get`, `status` |

- `export_image` draws the export in the MCP server. The TUI does not draw it. See [`export_image`](exports.md#export_image).
- The socket methods with "None" in the [MCP tool column](socket-api.md#methods) have no tool. The resource `tiza://scene` gives the result of `scene.get`.

## Arguments

The fields of each annotation type are in [Annotations](document-format.md#annotations). The tables in this section give the names that the tools use.

### Anchor

`at`, `from` and `to` take an anchor object. An anchor gives one place in the code.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `find` | string | No default | The exact text to find in the code, for example `"takes(s)"`. |
| `line` | integer | No default | The line number as the TUI shows it, from 1. |
| `col` | integer | `0` | The grapheme on the line, from 0, when the anchor has no `find`. |
| `occurrence` | integer | `1` | The match of `find` to use, from 1. |

- Give `find`, or `line`, or both. Without them, the tool fails with `invalid_argument`.
- `occurrence` counts the matches in reading order.
- With `line` and `find`, Tiza looks for the match of `occurrence` on that line first.
- When that line does not have it, Tiza looks in all the code. It then counts again from the top of the code.
- When the text is not in the code, the tool fails with `text_not_found`. Do not guess a column.
- For `highlight` and `add_mark`, the anchor gives a span. With `find`, the span is the text. Without `find`, the span goes from `col` to the end of the line.
- For `add_arrow`, the anchor gives an [arrow end](document-format.md#arrow-end): the first grapheme of the text, or `col`.
- The socket also takes `end` in an anchor. See [Differences from the MCP tools](socket-api.md#differences-from-the-mcp-tools).

### Arguments of all the add tools

| Field | Type | Default | Meaning |
|---|---|---|---|
| `step` | integer | The current step, or 1 at step 0 | The step at which the annotation appears, from 1 to 999. |
| `color` | string | No default | A [color name](document-format.md#color-names) or `#rrggbb`. |
| `id` | string | `a1`, `a2` and so on | The id of the new annotation. |

- A `step` of 0 becomes step 1.
- A `step` after 999 fails with `invalid_argument`.
- An annotation at a step after the last step makes that step.
- An arrow without a `color` gets the next color of the palette.
- An `id` that the document has already fails with `duplicate_id`. Give your own `id`. Then a second try of the same call does not add a copy.
- `add_spotlight` has no `color`.
- An unknown color name fails with `invalid_argument`.

### Range

`add_box`, `add_callout` and `add_spotlight` take a range of lines.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `from_line` | integer | No default | The first line of the range. |
| `to_line` | integer | `from_line` | The last line of the range. |
| `find_start` | string | No default | A text on the first line of the range. |
| `find_end` | string | No default | A text on the last line of the range. |

- Give `from_line`, or give `find_start` and `find_end` together. Do not give the two forms in one call.
- `find_start` is the first match in the code. `find_end` is the first match after `find_start`, on the same line or on a later line.
- Thus a `find_end` of `}` is often the brace of an inner block. To put a box around a whole function, give its lines.
- A marker that matches nothing fails with `text_not_found`. The message names the marker.
- The annotation keeps the two texts in `markers`.
- `add_spotlight` can have no range when it has `ids`.

### Arguments of each add tool

| Tool | Required | Other arguments |
|---|---|---|
| `add_box` | A range | `label`, `border`, `corners`, `fill` |
| `add_arrow` | `from`, `to` | `label`, `stroke`, `route`, `head_start`, `head_end` |
| `add_note` | `line`, `text` | No other argument |
| `add_callout` | A range, `text` | `title`, `placement`, `width`, `border`, `corners`, `fill` |
| `highlight` | `at` | `label`, `emphasis` |
| `add_mark` | `kind`, `at` | `label`, `style`, `emphasis` |
| `add_spotlight` | A range or `ids` | `ids`, `until` |

- `border` and `stroke` take a [stroke value](document-format.md#values-of-annotation-fields). The default is `solid`.
- `corners` and `route` take `rounded` or `sharp`. The default is `rounded`.
- `head_start` takes a [head value](document-format.md#values-of-annotation-fields). The default is `none`.
- `head_end` takes a head value. The default is `arrow`. With `none` at the two ends, the arrow is a plain line.
- `fill` is `true` or `false`. A box has `true` as the default, and a callout has `false`.
- `emphasis` takes an [emphasis value](document-format.md#values-of-annotation-fields). The default is `fill`.
- `placement` takes `margin`, `below` or `auto`. The default is `margin`.
- `width` is the maximum width of the text of a callout, in columns. The text wraps at that width.
- The block is the widest line of text plus 4 columns.
- In the margin, `width` goes from 8 to the width of the margin minus 5. The default is 34.
- Below the code, `width` is at most the room under the code. The default is all that room.
- In the margin, a long title can make the block wider. Below the code, Tiza cuts a long title.
- `kind` is a [kind](document-format.md#kinds), the name of a style, or any name when `style` has a `glyph`.
- `style` is a [mark style](document-format.md#mark-style) for this mark only: `glyph`, `legend` and `color`.
- `until` is the last step at which a spotlight is active. Without it, the spotlight is active only at its `step`.

### `define_style`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | Required | The name of the style, which you then give as the `kind` of a mark. |
| `glyph` | string | No default | The sign before the legend. |
| `legend` | string | No default | The text after the glyph. |
| `color` | string | No default | A [color name](document-format.md#color-names) or `#rrggbb`. |

- A new style needs a `glyph`, unless its name is a built-in kind.
- When the style exists, the tool changes only the fields that you give.

### `update_annotation`

`id` is required. Each other argument changes one part of the annotation. The annotation keeps all the parts that you do not give. The "Applies to" column replaces Type and Default, because every argument is optional.

| Field | Applies to | Meaning |
|---|---|---|
| `step` | All types | The new step, where 0 becomes 1. |
| `color` | All types | The new color, or an empty string for the automatic color. |
| `label` | Box, arrow, highlight, mark | The new label, or an empty string to remove it. |
| `label` | Note | The new text of the note. An empty string keeps the text. |
| `label` | Callout | The new title, or an empty string to remove it. |
| `file` | All types | The file of the annotation, as a `code.link` names it, or `null` for the main file. |
| `at` | Highlight, mark, note | A new anchor. |
| `from`, `to` | Arrow | A new start or end. |
| `from_line` | Box, callout, spotlight, note | A new first line, or the new line of a note. |
| `to_line` | Box, callout, spotlight | A new last line. |
| `stroke`, `route`, `head_start`, `head_end` | Arrow | A new line style. |
| `border`, `corners`, `fill` | Box, callout | A new border, new corners or a new fill. |
| `emphasis` | Highlight, mark | A new emphasis. |

- An argument that does not apply to the type of the annotation fails with `invalid_argument`. `color` and `label` are not in this rule.
- A spotlight stores a `color` and gives no error. `add_spotlight` has no `color`.
- A spotlight ignores `label` and gives no error. Nothing changes.
- An argument that the tool does not know fails. The reply names the arguments that it knows. `text` is not in this rule: see the next items.
- To change the text of a note, give `label`. For a callout, `label` changes the title.
- No tool changes the text of a callout. The socket method `annotation.update` can do it with `shape`. See [Differences from the MCP tools](socket-api.md#differences-from-the-mcp-tools).
- A `text` argument fails with a message that tells you to use `label`.
- A new place keeps the type, the kind, the style and the label of the annotation.
- A new `from_line` or `to_line` removes the `markers` of the annotation.

### Other tools

| Tool | Arguments | Meaning |
|---|---|---|
| `delete_annotation` | `id` | The id of the annotation to delete. |
| `clear_annotations` | `step` | The step to clear. Leave out `step` to clear every annotation. |
| `delete_move` | `index` | The index of the move, from 1, as `list_moves` gives it. |
| `set_step` | `step` | The step to show, where a step after the last step shows the last step. |
| `goto_step` | `id` | The `id` of a named step, or a step number as `4` or `"4"`. |
| `set_focus` | `from_line`, `to_line` | The range to keep in focus, or no line to clear the focus. |
| `point_at` | `line`, `find`, `from`, `to` | The line or the range that the live pointer shows. |
| `set_theme` | `theme` | `dark`, `light`, `terminal`, `auto` or `toggle`. |
| `set_follow` | `enabled` | `true` turns follow mode on, and `false` turns it off. |
| `render_text` | `width` | The width of the take in columns, from 20 to 400, or the width of the code area of the screen. |
| `check` | `width` | The width at which `check` measures the labels, from 20 to 400. The default is the `width` of the config file, else 100. |

- The presenter can undo `clear_annotations` with `u`. The reply gives `removed`, `remaining` and `revision`.
- `clear_annotations` refuses `"step": null` and an unknown key. In its input schema, `step` is an integer, and the schema has `"additionalProperties": false`.
- `add_move` takes the fields of a move. See [Moves](document-format.md#moves). `action` has `code.patch` as the default.
- `export_image` has its own table. See [`export_image`](exports.md#export_image).
- `point_at` takes one of these forms: `line`; `line` and `find`; `find`; or `from`, with `to` as an option.
- `find` alone must be on one line of the code on screen.
- `from` without `to` points at the line `from` only.
- `set_theme` also takes the aliases of [Agents and themes](themes.md#agents-and-themes).
- `set_theme` with an empty string changes between dark and light. The theme name is not case-sensitive.
- `goto_step` with an id that no named step has fails with `not_found`.
- `goto_step` with a step after the last step fails with `invalid_argument`. It does not show the last step, as `set_step` does.
- `goto_step` with an `id` that is not a string or a whole number fails with `invalid_argument`.
- `set_focus` with `from_line` only puts one line in focus.
- `set_focus` with `to_line` only fails with `invalid_argument`.
- When the first line of the range is outside the code, `set_focus` fails with `out_of_range`.
- A last line after the end of the code becomes the last line.
- `next_step` after the last step goes to step 0 when the document has `"loop": true`.
- `set_follow` fails when the code is embedded and no followed file is known. See [Agents and follow mode](../guides/follow-the-editor.md#agents-and-follow-mode).

## Replies

Each tool replies with text. The text is JSON, except for the tools in this list:

- `get_code` gives the code, with a line number before each line. When a `code.link` shows a file, the first line is `# VIEW`. `VIEW` is the file, or `FILE:A-B` with a window.
- `list_annotations` gives the step label first, as `# LABEL`, for example `# step 3/7 · Shared borrow`. It leaves out this line when the label is empty.
- Then `list_annotations` gives one line for each annotation, or `(no annotations)`.
- `render_text` gives the take. A second block of text names its width and tells you to give `check` the same width.
- The add tools and `update_annotation` give one sentence before the JSON.

In `list_annotations`, a line ends with `(FILE)` when the annotation belongs to a linked file. A line says `(unanchored)` when a move hides the annotation. A move that does not match the code any more comes last, as `move #N … (stale: WHY)`.

In a text reply, a control character other than a tab or a line break shows as its picture, for example `␛`, or as `�`. In a JSON reply, a control character is an escape, for example `\u001b`. DEL and a bidi control are escapes too. See [Control and invisible characters](characters.md).

### `status`

`status` gives the result of the socket method `status`, plus a `bridge` object about the MCP server.

| Field | Meaning |
|---|---|
| `version` | The version of the server. |
| `build` | The git commit of the server, as `tiza --version` gives it. |
| `binary_rebuilt` | `true` when the `tiza` binary on disk is newer than the server that runs. |
| `instance_build` | The commit of the instance, when it is not the commit of the server. |
| `hint` | What to do: connect the client again, and start the TUI again when its build is not the same. |

- `binary_rebuilt`, `instance_build` and `hint` are in the reply only when they apply.
- The fields of the socket method are in [Fields of `status`](socket-api.md#fields-of-status).

### Reply of an annotation tool

`add_box`, `add_arrow`, `add_note`, `add_callout`, `highlight`, `add_mark`, `add_spotlight` and `update_annotation` reply with the place of the annotation against the screen. The socket methods `annotation.add` and `annotation.update` give the same JSON.

| Field | Meaning |
|---|---|
| `id` | The id of the annotation. |
| `revision` | The revision of the document after the change. |
| `step` | The step of the annotation. |
| `step_title` | The title, or the id, of that step, only when it is a named step. |
| `shown_step` | The step on screen when Tiza applied the change. |
| `visible` | `true` when Tiza draws the annotation at `shown_step`. |
| `hidden` | Why Tiza does not draw the annotation, only when `visible` is `false`. |
| `clipped` | What happens to the label, only when the annotation is visible and its label is not complete. |

`visible` is `true` when all these conditions are true:

- The step is reached.
- The file is on screen.
- The anchor is kept.
- The lines are in the window.
- For a note or a callout, the take at the width of the screen must also have room for the text.
- In supervised mode, Tiza makes the reply when the presenter accepts the change. Thus `shown_step` can differ from the step that you saw.
- The reply tells the state at one moment. The screen can change after it.
- A hidden annotation is not an error. Do not add it again.

The sentence before the JSON has one of these forms:

```text
ID VERB at step N; visible now (the view is at step S).
ID VERB at step N; not visible now: the view is at step S and HIDDEN.
```

- `VERB` is `added` or `updated`.
- When the step has a title, `step N` becomes `step N (TITLE)`.
- When the label is not complete, `; CLIPPED` comes before the period of the first form.
- When the annotation appears at the next step, the second form ends with `(next_step shows it)`. When it appears at a later step, it ends with `(set_step N shows it)`.

For example:

```text
a18 added at step 5; not visible now: the view is at step 0 and it appears at step 5 (set_step 5 shows it).
```

The `hidden` field has one of these messages.

| Message | When |
|---|---|
| `it appears at step N` | The step on screen is an earlier step. |
| `its file FILE is not shown at step N (SHOWN is)` | A `code.link` shows another file at that step. |
| `its lines are outside the code shown at step N (A-B)` | The lines are outside the window of a `code.link`. |
| `it lost its anchor at step N (unanchored: no line of it survived a patch)` | A move removed every line of the annotation. |
| `a spotlight shows only at its step N` | A spotlight is active at one step, and the step on screen is after it. |
| `a spotlight shows from step A to B` | The step on screen is after the `until` of a spotlight. |
| `it is not drawn: no room for its text at W columns (check at width W lists it)` | The take at the width of the screen has no room for a note or a callout. |

The `clipped` field has one of these messages.

| Message | When |
|---|---|
| `its label is cut with … at W columns (check at width W lists it)` | Tiza cuts the label at the width of the screen. |
| `its label is not drawn: no room at W columns (check at width W lists it)` | Tiza does not draw the label at the width of the screen. |

`W` is the width of the code area of the screen. `check` at that width gives the same result.

- The width of the code area is the terminal width less the side padding, from 20 to 400. For example, it is 98 in a terminal of 100 columns with the default padding.
- Before the instance knows the size of its terminal, the width is 100.

To make a label fit, make it shorter, or ask the presenter for a wider terminal.

## Resources

The server gives these resources. A client that supports resources can attach them as context without a tool call.

| Resource | Type | What it holds |
|---|---|---|
| `tiza://scene` | `application/json` | The live document as JSON, as the socket method `scene.get` gives it. |
| `tiza://code` | `text/plain` | The code on screen with line numbers, as `get_code` gives it without its `# VIEW` line. |
| `tiza://render` | `text/plain` | The take at the current step as plain text, as `render_text` gives it without its width line. |
| `tiza://render.md` | `text/markdown` | The same take in a fenced `text` block, ready to paste. |

- Each read asks the live instance. The server keeps no copy.
- When no instance listens, a read fails as a tool call fails.
- `tiza://render.md` starts with `**NAME** (step N)` when the instance shows a file. `NAME` is the name of that file.
- `tiza://scene` is a read. To change the document, use [`scene.put` or `scene.patch`](socket-api.md#sceneput-and-scenepatch) on the socket.

## Errors

A tool fails in one of three ways.

- The server returns a JSON-RPC error. The error has a code and a message.
- The tool returns a result with `isError: true`. The text of the result is the message.
- The instance refuses the request. The server then returns a JSON-RPC error with the message and the socket code in parentheses, for example `annotation zz does not exist (not_found)`.

The socket codes are in [Error codes](socket-api.md#error-codes).

| Message | JSON-RPC code | When |
|---|---|---|
| `MESSAGE (CODE)` | `-32602` | The instance refused the request. |
| `` No Tiza instance is listening on PATH (ERROR). Ask the user to open `tiza <file>` in a terminal, or pass --socket/--session to the MCP server. `` | `-32603` | The server did not get a reply from the instance. |
| `The Tiza instance on PATH did not answer within 75 s: it is busy or stopped. Ask the user to look at the Tiza terminal.` | `-32603` | The instance got the request, but it did not reply in time. |
| `The Tiza instance on PATH closed the connection before it answered: it possibly quit. Ask the user to look at the Tiza terminal.` | `-32603` | The instance closed the connection before it replied. |
| `unknown border (use solid, dashed, dotted, thick, double) (invalid_argument)` | `-32602` | `border` has a value that Tiza does not know. |
| `unknown corners (use rounded, sharp) (invalid_argument)` | `-32602` | `corners` has a value that Tiza does not know. |
| `unknown stroke (use solid, dashed, dotted, thick, double) (invalid_argument)` | `-32602` | `stroke` has a value that Tiza does not know. |
| `unknown route (use rounded, sharp) (invalid_argument)` | `-32602` | `route` has a value that Tiza does not know. |
| `unknown start head (use none, arrow, dot, bar, open) (invalid_argument)` | `-32602` | `head_start` has a value that Tiza does not know. |
| `unknown end head (use none, arrow, dot, bar, open) (invalid_argument)` | `-32602` | `head_end` has a value that Tiza does not know. |
| `unknown emphasis (use fill, underline, brackets, strike) (invalid_argument)` | `-32602` | `emphasis` has a value that Tiza does not know. |
| `unknown placement (margin, below, auto) (invalid_argument)` | `-32602` | `placement` has a value that Tiza does not know. |
| `` update_annotation has no `text`. Use `label` to change the text of a note or the title of a callout. To change the text of a callout, remove it and add it again (invalid_argument) `` | `-32602` | `update_annotation` got `text`. |
| `width=N: use a number between 20 and 400 (invalid_argument)` | `-32602` | The `width` of `check` is outside the limits. |
| `tool not found` | `-32602` | The client called a tool that the server does not have. |
| `unknown resource; available: tiza://scene, tiza://code, tiza://render, tiza://render.md` | `-32002` | The client read a resource that the server does not have. |
| `internal error: the WHAT call failed with a bug in Tiza (a panic; its message is on the MCP server's stderr). Please report it; the live instance is unaffected` | `-32603` | A bug in the server stopped the call. |
| `failed to deserialize parameters:` | Result with `isError` | An argument is wrong. |

- `ERROR` is the error of the system, for example `No such file or directory (os error 2)`.
- The message `No Tiza instance is listening` comes for each of these cases:
  - The server cannot connect to the socket.
  - The server cannot write the request.
  - The reply is not JSON.
- `WHAT` is `tool NAME` for a tool call, and `resource` for a resource read.
- `failed to deserialize parameters:` comes when an argument has the wrong type. It also comes when a required argument is missing or `null`.
- `failed to deserialize parameters:` also comes when `update_annotation` or `clear_annotations` gets an argument that it does not know. `text` of `update_annotation` gets its own message.
- `failed to deserialize parameters:` ends with the error of the parser, for example ``missing field `text` ``.
- The messages of `export_image` are in [Errors of `export_image`](exports.md#errors-of-export_image).
- The server checks `border`, `corners`, `stroke`, `route`, the heads, `emphasis`, `placement` and the `width` of `check` itself. It sends no request for a bad value.
- These refusals of the server end with `(invalid_argument)`, as a refusal of the instance does.
- An error about a value from a fixed set, such as `color` or `border`, does not repeat the wrong value. It gives the valid values. An error about a value of the wrong type repeats the value, cut at 40 characters.
- The `width` of `render_text` goes to the instance. The instance refuses a bad width with `invalid_argument`.
- Only `update_annotation` and `clear_annotations` refuse an argument that they do not know. The other tools ignore an unknown argument at the top level.
- In `add_move`, a `rebind` entry with a key that Tiza does not know fails. For the socket, see [Keys that Tiza does not know](socket-api.md#keys-that-tiza-does-not-know).
- Do not try again in a loop after `rejected` or `internal`. See [Supervised mode](../agents/supervised-mode.md).

## See also

- [Demo scenarios](../demos/scenarios.md): ways to use the tools with an agent, for example a codebase tour or a review.
- [A tour of tally](../demos/tour-of-tally.md): a tutorial that builds a tour of four files with the tools.
