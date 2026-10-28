# Document format

This page describes the `.tiza` document: the JSON file that holds the code reference, the annotations, the moves and the named steps. Use it to read or write a document by hand.

## The document and its code

A `.tiza` file is a JSON document. With autosave on, Tiza saves it next to the source file. For example, `tiza src/main.rs` saves to `src/main.rs.tiza`. See [Saving](../concepts/saving.md).

The document holds its code in one of two forms:

- **A code reference** names the source file by path: `"code": {"file": "main.rs", "lines": "12-48"}`. The path is absolute or relative to the `.tiza` file. Thus the document always shows the current code of the file.
- **Embedded code** is a copy of the code inside the document, as an array of lines. `:embed` makes this copy. It also copies every file that a `code.link` move shows.

| Command | What it does |
|---|---|
| `:embed` | Copies the code and every linked file into the document. |
| `:ref PATH[:A-B]` | References a file again, at any step. |
| `:link` | References the source file again. |
| `:link PATH[:A-B]` at step 0 | Does the same as `:ref PATH[:A-B]`. |
| `:link PATH[:A-B]` at step 1 or later | Adds a `code.link` move at the current step (see [Several files](#several-files)). |

`:ref`, and `:link` when it references a file, refuse two cases:

- The file has other code, and a move is applied. Go back to step 0 first.
- A `code.link` move shows the file already, because its annotations belong to that view.

## Examples

Two examples show moves and views:

- `tiza examples/ownership-moves.rs.tiza` types a helper function. Later it replaces the function with a method call.
- `tiza examples/stack-and-queue.tiza --no-save` shows a stack and a queue in two views. It changes from one view to the other and back. Each view has its own annotations.

## Top-level fields

| Field | Type | Default | Meaning |
|---|---|---|---|
| `schema` | integer | Required | The version of the format, `1`. |
| `source` | object | Required | The language and the first line of the code (see [`source`](#source)). |
| `code` | array or object | Required | The embedded code as an array of lines, or a [file reference](#file-reference). |
| `follows` | object | No default | A [file reference](#file-reference) to the file that follow mode reloads when the document has embedded code. |
| `annotations` | array | `[]` | The list of [annotations](#annotations). |
| `step` | integer | `1` | The step that the document opens at, from 0 to the last step. |
| `moves` | array | `[]` | The list of [moves](#moves). |
| `views` | object | `{}` | The code of each linked file that `:embed` stores, as an array of lines, with the `file` of its `code.link` as the key. |
| `steps` | array | `[]` | The list of [named steps](#named-steps). |
| `loop` | `true` or `false` | `false` | When `true`, the step after the last step is step 0. |
| `styles` | object | `{}` | The styles of the document, a [mark style](#mark-style) for each name. |
| `title` | string | No default | The title of the document. |
| `theme` | `"dark"`, `"light"` or `"terminal"` | No default | The theme of the document. |
| `theme_overrides` | object | No default | Colors that apply over the `[theme]` table of the config file (see [Theme overrides](#theme-overrides)). |
| `padding` | array of two integers | No default | The blank rows and columns around the code, as `[vertical, horizontal]`. |
| `margin` | integer | No default | The maximum width of the right margin in columns. Notes wrap at this width. Tiza cuts labels, legends and long notes there. |
| `numbers` | `true` or `false` | `numbers` of the config file, else `true` | When `false`, Tiza does not show line numbers. |
| `effects` | object | `effects` of the config file, else no effects | Experimental. How the annotations of a step come in. See [Enter effects](#enter-effects-experimental). |

- `views` holds the full code of a linked file. The window stays in the move.
- When a linked file is not in `views`, Tiza reads it from disk.
- A saved `step` after the last step opens at the last step.
- `terminal` uses the colors of the terminal. It works only in the TUI. An export uses `dark` or `light`.
- Tiza ignores a top-level key that it does not know, and does not save it.

### `source`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `language` | string | `""` | The language of the code, as an extension or a name, for example `rs`, `ts` or `py`. |
| `first_line` | integer | `1` | The number of the first line shown, at most 1,000,000,000. |
| `path` | string | No default | The path of the original file. |

With a code reference, Tiza sets `first_line` from `lines`. It also sets a missing `path` and an empty `language` from the file.

### File reference

`code` and `follows` use this object.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `file` | string | Required | The file that holds the code, as a path that is absolute or relative to the `.tiza` file. |
| `lines` | `"12-48"`, `"12..48"` or `"7"` | The whole file | The lines of the file that the document shows. |

## Annotations

A new annotation from the TUI appears at the current step. A socket or MCP request can name its own step. At step 0, a new annotation goes to step 1. This rule also applies to a socket or MCP request that names step 0.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | Required | The short public id, unique in the document, for example `a1`. |
| `step` | integer | `1` | The step at which the annotation appears, at most 999. |
| `type` | string | Required | The annotation `type`, one of the seven values under this table. |
| `color` | string | No default | A [color name](#color-names) or `#rrggbb`. |
| `file` | string | The document's own file | The main file or a linked file that the annotation belongs to (see [Several files](#several-files)). |
| `markers` | object `{start, end}` | No default | The two texts that Tiza uses to find a range again after the code changes. |

The values of `type` are:

- `highlight`
- `mark`
- `box`
- `arrow`
- `note`
- `callout`
- `spotlight`

`from_line` and `to_line` give the range of a box, a callout or a spotlight. `markers` only finds that range again. `start` is the first match in the code, and `end` is the first match at or after it.

Tiza ignores a key of an annotation that it does not know, and does not save it.

### Annotation types

Each `type` adds its own fields.

#### `highlight`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `span` | object | Required | The part of the line to highlight, a [span](#span-and-arrow-end). |
| `label` | string | No default | The text next to the code. |
| `emphasis` | string | `"fill"` | How Tiza draws the span, one of the [emphasis values](#values-of-annotation-fields). |

#### `mark`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `kind` | string | Required | The meaning of the mark, one of the [kinds](#kinds). |
| `span` | object | Required | The part of the line to mark, a [span](#span-and-arrow-end). |
| `label` | string | No default | The text after the legend. |
| `style` | object | No default | A [mark style](#mark-style) for this mark only. |
| `emphasis` | string | `"fill"` | How Tiza draws the span, one of the [emphasis values](#values-of-annotation-fields). |

#### `box`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `from_line` | integer | Required | The first line of the box. |
| `to_line` | integer | Required | The last line of the box, not before `from_line`. |
| `label` | string | No default | The text on the box. |
| `border` | string | `"solid"` | The line of the border, one of the [stroke values](#values-of-annotation-fields). |
| `corners` | `"rounded"` or `"sharp"` | `"rounded"` | The shape of the corners. |
| `fill` | `true` or `false` | `true` | When `true`, Tiza tints the inside of the box. |

#### `arrow`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `from` | object | Required | The start of the arrow, an [arrow end](#span-and-arrow-end). |
| `to` | object | Required | The end of the arrow, an [arrow end](#span-and-arrow-end). |
| `label` | string | No default | The text on the arrow. |
| `stroke` | string | `"solid"` | The line of the arrow, one of the [stroke values](#values-of-annotation-fields). |
| `route` | `"rounded"` or `"sharp"` | `"rounded"` | The shape of the corners. |
| `heads` | object `{start, end}` | `{"start": "none", "end": "arrow"}` | The heads at the two ends, each one of the [head values](#values-of-annotation-fields). |

#### `note`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `line` | integer | Required | The line of the note. |
| `text` | string | Required | The text of the note, which is not blank. |

#### `callout`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `from_line` | integer | Required | The first line that the callout explains. |
| `to_line` | integer | `from_line` | The last line that the callout explains, not before `from_line`. |
| `text` | string | Required | The text of the callout, which is not blank. |
| `title` | string | No default | The title of the callout. |
| `placement` | `"margin"`, `"below"` or `"auto"` | `"margin"` | Where Tiza draws the callout. |
| `width` | integer | Depends on `placement` | The width at which Tiza wraps the text, in columns. |
| `border` | string | `"solid"` | The line of the border, one of the [stroke values](#values-of-annotation-fields). |
| `corners` | `"rounded"` or `"sharp"` | `"rounded"` | The shape of the corners. |
| `fill` | `true` or `false` | `false` | When `true`, Tiza tints the inside of the callout. |

#### `spotlight`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `from_line` | integer | Required when there are no `ids` | The first line that the spotlight does not dim. |
| `to_line` | integer | `from_line` | The last line that the spotlight does not dim, not before `from_line`. |
| `ids` | array of ids | Required when there is no `from_line`, else `[]` | The ids of annotations in the document that the spotlight does not dim, also outside its lines. |
| `until` | integer | No default | The last step at which the spotlight is active. |

#### Values of annotation fields

- Stroke values: `solid`, `dashed`, `dotted`, `thick`, `double`.
- Head values: `none`, `arrow`, `dot`, `bar`, `open`.
- Emphasis values: `fill`, `underline`, `brackets`, `strike`.

Each value has this meaning:

- `emphasis`: `fill` tints the span.
- `emphasis`: `underline` draws a row under the span.
- `emphasis`: `brackets` draws `⌈ ⌉` beside the span.
- `emphasis`: `strike` draws a line through the span.
- `emphasis`: `fill` is the default. Tiza does not write it to the file.
- `placement`: `margin` puts the callout in the right margin.
- `placement`: `below` puts the callout in its own rows after `to_line`.
- `placement`: `auto` uses the margin when the terminal has room.
- `width`: in the margin, the default is 34. Tiza keeps the width from 8 to 5 columns less than the margin.
- `width`: below the code, the default is the room below the code. Tiza keeps the width in that room.
- `width`: the block is as wide as its widest line of text, plus 4 columns for the borders and the padding.
- `title`: in the margin, a long title makes the block wider. Below the code, Tiza cuts a long title in the border.

#### Rules for annotations

- A spotlight is active from its `step` to `until`. Without `until`, it is active only at its `step`.
- A spotlight dims the code outside its lines. It does not dim the code of the annotations in its `ids`.
- A spotlight also dims each annotation that does not touch its lines and that is not in its `ids`.
- An annotation touches the lines of a spotlight when one or more of its lines are in the range. The lines of a box or a callout go from `from_line` to `to_line`. The lines of an arrow go from one end to the other. Thus an arrow that goes across the range touches it.
- A spotlight with only `ids` shows only the code of those annotations. It dims all the other annotations.
- When two or more spotlights are active at one step, Tiza uses the lines and the `ids` of all of them.
- Tiza refuses a blank `text`, and a range that ends before it starts, when you add an annotation.
- An annotation whose lines are not in the code stays in the file. Tiza does not draw it, and `tiza check` gives an error for it.

### Span and arrow end

A span covers part of one line. An arrow end is one place in the code.

#### Span

| Field | Type | Default | Meaning |
|---|---|---|---|
| `line` | integer | Required | The line of the span. |
| `start` | integer | Required | The first grapheme (one character as you see it), counted from 0. |
| `end` | integer | Required | The grapheme after the last one, more than `start` and at most the length of the line. |

#### Arrow end

| Field | Type | Default | Meaning |
|---|---|---|---|
| `line` | integer | Required | The line of the arrow end. |
| `col` | integer | Required | The grapheme of the arrow end, counted from 0, at most the length of the line. |

### Kinds

The `kind` of a mark is one of these:

- A built-in kind: `info`, `note`, `warn`, `error`, `ok`, `question`, `important`, `read`, `write`, `move`, `new`, `drop`, `scope`, `flow`, `todo`.
- An alias of a built-in kind, from the table under this list.
- The name of a style in `styles`. A style with the name of a built-in kind replaces that kind.
- Any name, when the mark has a `style` with a `glyph`. The name is then the default legend.

When you add a mark, Tiza refuses any other `kind`. A loaded file keeps such a mark, and `tiza check` reports it. Case does not matter for a built-in kind or an alias.

| Alias | Kind |
|---|---|
| `borrow`, `ref`, `&` | `read` |
| `borrow_mut`, `borrowmut`, `mut`, `&mut` | `write` |
| `lifetime`, `life`, `'a` | `scope` |
| `free`, `delete` | `drop` |
| `alloc`, `create` | `new` |
| `bug` | `error` |
| `warning` | `warn` |
| `tip` | `info` |

### Mark style

Each entry of `styles`, and the `style` of a mark, has these fields. The `style` of a mark replaces each field that it gives.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `glyph` | string | The glyph of the kind | The sign before the legend, which should be one or two columns wide. |
| `legend` | string | The legend of the kind | The text after the glyph, followed by `: ` and the `label` of the mark. |
| `color` | string | The color of the kind | A [color name](#color-names) or `#rrggbb`. |

### Color names

| Name | Aliases |
|---|---|
| `orange` | `rust`, `naranja`, `accent` |
| `blue` | `azul` |
| `green` | `verde` |
| `purple` | `morado`, `violeta` |
| `pink` | `magenta`, `rosa` |
| `cyan` | `teal`, `cian` |
| `yellow` | `amarillo` |
| `red` | `rojo` |
| `gray` | `grey`, `gris` |

- Case does not matter. A hex color has exactly six digits, so `#rgb` is not a color of an annotation.
- Tiza refuses an unknown color when you add an annotation. In a file, Tiza draws an annotation with an unknown color in its default color.
- `accent` gives the accent color of the theme. An override of `orange` does not change `accent`.
- An arrow without a color takes the next color of a palette of seven colors: `orange`, `blue`, `green`, `purple`, `cyan`, `pink`, `yellow`. The eighth arrow starts the palette again.
- A mark without a color uses the color of its kind or its style. For example, `move` is pink and `read` is cyan.
- A box without a color uses the box color of the theme. A note or a callout uses the note color, and a highlight or a spotlight uses the accent color.

### Arrow styles

The `stroke` and `heads` fields change the look of an arrow. For example, `examples/ownership.rs.tiza` shows a shared borrow with a `dashed` arrow and a mutable borrow with a `thick` arrow. In `:arrow`, `heads=none,none` gives an arrow with no heads.

### Theme overrides

`theme_overrides` is an object. Each key is a role, a color name or a theme name.

| Key | Value |
|---|---|
| A role, from the list under this table | A color value |
| A color name, from the list under this table | A color value |
| `dark`, `light` or `terminal` | An object with the same keys, which applies only with that theme |

- Roles: `bg`, `fg`, `muted`, `dimmed`, `accent`, `rail`, `box_line`, `box_bg`, `arrow`, `note`, `highlight_bg`, `status_bg`, `status_fg`, `cmd_bg`, `error`.
- Color names: `orange`, `blue`, `green`, `purple`, `pink`, `cyan`, `yellow`, `red`, `gray`.

A color value is one of these:

- `#rrggbb` or `#rgb`.
- `rgb(r, g, b)`.
- An ANSI name, for example `red` or `bright-blue`. It gives a color of the terminal palette.
- `ansiN`, a color of the terminal palette from 0 to 255.
- `reset`, `default` or `none`, the default color of the terminal.

A color name of an annotation, such as `orange`, is not a color value. `red`, `blue` and the other names that are also ANSI names give the colors of the terminal. In an export, a token with a color of the terminal palette uses the color of the `dark` or `light` theme, and Tiza prints a note. The keys are the same as in the `[theme]` table of the config file.

## Moves

A move changes the code or the view at a step. The `moves` list of a document holds them.

- `n` applies the moves of the next step. `p` reverts them.
- Tiza always computes the document again from the code of step 0. Thus the document returns to the same state after any number of steps forward and back.

Every move has `at_step` and `action`. A `code.patch` move can also have `rebind`:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `at_step` | integer | Required | The step of the move, from 1 to 999. |
| `action` | string | Required | The action of the move, one of the actions in the table under this one. |
| `rebind` | object | `{}` | The new place of each annotation that the move replaces or deletes (see [Annotations after a `code.patch` move](#annotations-after-a-codepatch-move)). |

| Action | What it does |
|---|---|
| `code.patch` | Inserts, replaces or deletes whole lines of the file shown at its step. |
| `code.link` | Shows another file from its step on (see [Several files](#several-files)). |
| `cursor` | Puts the cursor on a line. |
| `select` | Selects a span. |
| `wait` | Pauses before the annotations of the step appear. |

Several moves at one step run in the order of the list.

### Fields of a `code.patch` move

| Field | Type | Default | Meaning |
|---|---|---|---|
| `op` | `"insert"`, `"replace"` or `"delete"` | Required | The change to make. |
| `position` (insert) | string | Required when it is the target | For `insert`, the one line before which Tiza puts the content, up to one more than the last line. |
| `position` (replace, delete) | string | Required when it is the target | For `replace` and `delete`, the line range, `"7"` or `"7-9"`. |
| `after`, `before` | string | Required when it is the target | For `insert` only, a text in the line next to the new content. |
| `find` | string | Required when it is the target | For `replace` and `delete` only, a text in the one line to change. |
| `find_start`, `find_end` | string | Required together when they are the target | For `replace` and `delete` only, texts in the first line and the last line of the range. |
| `occurrence` | integer | No default | The match of `find`, `after`, `before` or `find_start` to use, from 1. |
| `end_occurrence` | integer | No default | The match of `find_end` to use, from 1. |
| `content` | string | Required for `insert` and `replace` | The new lines, with `\n` between them. |
| `typing` | `"instant"`, `"line"` or `"char"` | `"instant"` | How Tiza shows the new lines. |
| `speed_ms` | integer | `120` for `line`, `25` for `char` | The milliseconds for each line or each character, at most 3,600,000. |

- Give exactly one target: `position`, `after`, `before`, `find`, or `find_start` with `find_end`.
- An insert `position` is one line. Tiza refuses a range.
- A `delete` has no `content`. Tiza ignores a `\n` at the end of `content`.
- `find_end` must not come before `find_start`.

### Where a `code.patch` move goes

Tiza finds a target on the code as it is at the step of the move. The target must match exactly once, or `occurrence` names the match.

- A text target matches part of a line. A line that holds the text two times counts as two matches.
- When the text occurs more than once, add `"occurrence": N` to name the match. `N` starts at 1.
- Tiza counts the matches in reading order over the whole code at that step. `end_occurrence` does the same for `find_end`.

### Typing

With `"typing": "line"` or `"char"`, Tiza types the content on the screen.

- `speed_ms` is the time for each line or each character.
- The annotations of the step appear when the typing ends.
- Any key except Ctrl-C finishes the typing. Tiza uses that key only to finish the typing.
- Thus an `n` during the typing finishes the typing. It does not go to the next step. Press `n` again.

### Annotations after a `code.patch` move

You add an annotation on the code at its own step. A later move changes the lines. Tiza moves the annotation with them. It does not search for text.

- An insert or a delete moves the lines after it.
- A box that loses an edge line gets smaller.
- When a `replace` replaces a line, a note, a box, a callout or a spotlight on that line goes to the new line. This occurs only when the new content has a line at that position.
- When the new content has fewer lines, the extra old lines are gone. A note on one of them loses its line, and a box gets smaller.
- On a replaced line, a highlight, a mark or an arrow end has no text under its span. Tiza hides it as unanchored. Tiza never deletes it.
- Tiza also hides an annotation that loses every line.

`rebind` gives the new place of each annotation that the move replaces or deletes. Each key is the id of an annotation. Tiza finds the target of each entry on the code after the move. For example:

```json
"rebind": {"a4": {"at": {"find": "t.len()", "occurrence": 1}}}
```

| Annotation | Fields of a `rebind` entry |
|---|---|
| A highlight or a mark | `at`, a [point](#point) |
| A note | `at`, a [point](#point), or `from_line` |
| An arrow | `from`, `to` or both, each a [point](#point) |
| A box, a callout or a spotlight | `from_line`, `to_line` or both |

- Each field of an entry is optional, but an entry needs at least one. Tiza refuses a key that it does not know. It also refuses an empty entry for an annotation of the document.
- Only a `code.patch` move can have `rebind`.
- Tiza keeps an entry for an annotation that is not in the document, and ignores it. `tiza render` gives a warning for it, and `tiza check` names it.

`:list` marks a hidden annotation `! unanchored`. The socket method `scene.list` and the MCP tool `list_annotations` mark it `(unanchored)`. `tiza check` gives a warning for it, with its steps.

### Point

`select` and a `rebind` entry use this object for one point of the code.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `find` | string | Required when there is no `line` | The text to find in the code. |
| `occurrence` | integer | No default | The match of `find` to use, from 1. |
| `line` | integer | Required when there is no `find` | The line of the point, or the line where `find` searches (see the list under this table). |
| `col` | integer | Depends on the use (see the list under this table) | The grapheme of the point, counted from 0, at most the length of the line. |
| `end` | integer | The end of the line | In a `rebind` `at` with `line`, the grapheme after the span, at most the length of the line. |

- In a `select`, `find` searches only `line` when the point has one. Otherwise it searches the whole code.
- In a `select`, `find` must match exactly once, or the point must have `occurrence`.
- In a `rebind` entry, `find` searches `line` first, and then the whole code.
- In a `rebind` entry without `line`, `find` must match exactly once, or the point must have `occurrence`. With `line`, Tiza does not check the number of matches.
- In a `select`, `find` in `from` starts at the start of the text, and `find` in `to` ends at the end of the text.
- In a `select` without `col`, `from` is the start of the line and `to` is the end of the line. In a `rebind` entry, the default `col` is 0.

### Several files

A `code.link` move shows another file from its step on. Each file is a view, with its own annotations. For example:

```json
{"at_step": 2, "action": "code.link", "file": "src/queue.rs", "lines": "10-36"}
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `file` | string | Required | The file to show, as a path relative to the `.tiza` file. |
| `lines` | `"7"` or `"7-30"` | The whole file | The window of the file on the screen, starting at line 1. |

At step 2, `:link src/queue.rs:10-36` adds the same move.

- An annotation that you add while a file is shown belongs to that file. It gets a `"file"` field. Without `file`, it belongs to the document's own file.
- The lines of an annotation are the real line numbers of its file.
- `lines` is only a window that crops the screen. Tiza keeps a note outside the window. Tiza does not draw it.
- The same file with another window is the same view.
- When you link the document's own file again, its annotations return exactly as they were.
- A `code.patch` move changes the file shown at its step.
- `:embed` stores every linked file under `views`. Thus the document stays one `.tiza` file.
- A document fails to load when a linked file cannot be read. The error names the move and the path.

When a document has a `code.link` move or an annotation of another file, `:list` groups the annotations of each step by file. `scene.list` ends the line of an annotation of another file with that file. For example:

```
a4 [step 3] box L15-17 (src/queue.rs)
```

### Cursor, select and wait

#### `cursor`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `line` | integer | Required | A line of the shown file. |
| `col` | integer | `0` | The grapheme of the cursor, at most the length of the line. |

#### `select`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `from` | object | Required | The start of the selection, a [point](#point). |
| `to` | object | Required | The end of the selection, a [point](#point). |

#### `wait`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `ms` | integer | Required | The pause in milliseconds, from 1 to 3,600,000. |

- `n` applies these moves. `p` reverts them.
- A `code.link` move removes the cursor and the selection.
- `tiza render --step N` and the image exports draw the cursor and the selection immediately.
- The TUI, the GIF, MP4 and WebM exports and the `.tape` show the pause of a `wait`. A `.cast` adds the pause to the time of the step, except at the last step.
- A still export ignores a `wait`.

### Exports and moves

- `tiza render --step N` and `.cast` apply the moves immediately.
- A `.tape` runs the TUI, so the TUI types each typed move. A GIF, MP4 or WebM export shows the typing as frames.
- `tiza render --step N` after the last step writes a warning on stderr. It shows the last step.
- An export that records the steps (GIF, MP4, WebM, `.cast`, `.tape`) gives no such warning.
- A `.tape` holds a step for its typing and its `wait` moves, plus half a second. When `--interval` is longer, it holds the step for `--interval`.

## Named steps

Named steps, auto-advance and loop are automation. Use them for an unattended export, a screen that loops or an editor task that goes to the next step. They do not add speaker notes, a timer or a remote control.

For example:

```json
"steps": [{"step": 3, "id": "borrow", "title": "Shared borrow", "auto_advance_ms": 3000}]
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `step` | integer | Required | The number of the step that the entry names, from 0 to 999. |
| `id` | string | No default | The name that the goto commands use, under the `id` rules of this section. |
| `title` | string | No default | The text that the status bar and `status` show. |
| `auto_advance_ms` | integer | No default | The time in milliseconds before Tiza goes to the next step by itself, from 1 to 3,600,000. |

- An entry names its step by number, never by its place in the list. Thus `+` and `-` move an annotation to another step and change no entry.
- Entries can skip steps. Tiza saves them sorted by `step`.
- One step has one entry or none.

An `id` must obey these rules. The error for each rule is in [Messages](#messages).

- It is not empty. To give no id, leave the field out.
- It is unique.
- It is one word. A control character counts as a blank.
- It is not all digits, because a number always means a step number.
- It does not start with `-`, because `tiza goto -1` reads it as an option.

### Go to a named step

The goto commands go to a step exactly as `n` does from step 0. Tiza applies the moves and the linked files again. A typed step shows all its text. An unknown id is a `not_found` error.

| Command | What it does |
|---|---|
| `:goto ID` | Goes to the named step, or to a step number. |
| `step.goto {"id": "ID"}` | The socket method that does the same as `:goto ID`. |
| `goto_step` | The MCP tool that does the same as `:goto ID`. |
| `tiza goto ID` | Makes the live instance do the same as `:goto ID`. |
| `tiza next` | Makes the live instance go to the next step. |
| `tiza prev` | Makes the live instance go to the previous step. |

- `tiza next`, `tiza prev` and `tiza goto` need no JSON. They use the socket of the session, or `--socket` and `--session` as `tiza api` does.
- They print the step that the instance reached, for example `step 3/7 · Shared borrow`.
- When no instance runs, they exit with a non-zero code and an error. See [Messages](#messages).

The `status` reply has these fields about the step:

| Field | Type | Meaning |
|---|---|---|
| `step` | integer | The step on screen. |
| `max_step` | integer | The last step of the document. |
| `step_id` | string | The `id` of the step on screen. The reply leaves it out when the step has no `id`. |
| `step_title` | string | The `title` of the step on screen. The reply leaves it out when the step has no `title`. |
| `step_label` | string | The step and its title, for example `step 3/7 · Shared borrow`. |
| `auto_advance_ms` | integer | The `auto_advance_ms` of the step on screen. The reply leaves it out when the step has none. |
| `loop` | `true` | The document loops. The reply leaves it out when the document does not loop. |

Presentation mode has no status bar. Thus `n`, `p` and an auto-advance do not show the title in a take. A `:` command, such as `:goto ID`, shows its result on the last row for 2 seconds. That result can be `step N/M · TITLE`.

### Auto-advance

With `auto_advance_ms`, the TUI goes to the next step by itself. The time starts when the step is ready: the typing has finished, and each `wait` move has elapsed.

- A key during the typing only finishes the typing.
- An open `:` line, a `v` or `V` selection, the help, the list or the review holds the countdown until it closes.

Exports use `auto_advance_ms` in different ways:

- A GIF, animated WebP, MP4 or WebM export holds such a step for `auto_advance_ms` instead of `--interval`.
- A `.tape` holds such a step for its typing plus `auto_advance_ms`, and sends no `n` for the next step.
- A `.cast` ignores `auto_advance_ms`. It holds each step for `--interval` plus its `wait` moves. The last step holds for `--interval` only.

### Loop

With `"loop": true`, `n` after the last step goes back to step 0. An auto-advance on the last step does the same. Without `loop`, Tiza stays on the last step.

### Entries that Tiza does not use

Tiza never drops these entries:

- An entry for a step after the last step.
- A key of an entry that Tiza does not know, for example a typo or the old `timer_min`.

Tiza keeps them in the file. It gives one warning for each entry, and the warning names all the unknown keys of that entry. The warning appears when you open the document. It also appears in `status.warnings` and in the output of `tiza render` and `tiza check`.

Only the entries of `steps` keep unknown keys. Tiza does not keep an unknown key at the top level or in an annotation.

## Enter effects (experimental)

This function is an experiment. It can change or go away.

An enter effect shows how the new annotations of a step come in when the step goes forward. The layout does not change. The effect only changes which cells of the final drawing show, and their colors.

```json
"effects": {"enter": "draw", "dim": "fade"}
```

| Field | Values | Default | Meaning |
|---|---|---|---|
| `enter` | `"draw"` or `"none"` | `"none"` | With `draw`, Tiza draws the line of each new annotation along its path. Then the text of callouts and labels fades in. |
| `dim` | `"fade"` or `"none"` | `"none"` | With `fade`, the cells that a new spotlight dims fade to the dimmed color. |

- Tiza draws a box or a callout clockwise from its top-left corner, an underline from left to right, and an arrow from its source.
- The code that an annotation colors fades in with its line. A highlight with no line fades in.
- The annotations of one step come in at the same time.
- The effect plays when the step goes forward: `n`, an auto-advance, a goto and the loop. It plays after the typing of the step.
- Any key ends the effect, and Tiza ignores that key. `p` shows the final drawing at once.
- The auto-advance countdown starts when the effect ends.
- A GIF, an animated WebP, an MP4 and a WebM show the effect as frames. A still, a text export and `render --step N` show the final drawing.
- `--effects` of the TUI and of `tiza render`, and `:effects`, win over the document. The document wins over `effects` in `[defaults]` of the [config file](config.md).
- `--effects` and `:effects` take `draw`, `dim`, `draw,dim`, `on` or `off`.
- With the `terminal` theme, a fade changes the color in one step at its middle.

## Rules the schema does not check

The [document schema](../../schema/tiza.schema.json) accepts some documents that Tiza refuses. Tiza does not load a document that breaks one of these rules:

- `schema` is at most `1`.
- Every `at_step` is 1 or more, because step 0 is the code with no change.
- No annotation, move or entry of `steps` names a step after 999.
- `wait.ms` is 1 or more.
- `wait.ms`, `speed_ms` and `auto_advance_ms` are at most 3,600,000 ms, one hour.
- With embedded code, `source.first_line` is at most 1,000,000,000.
- Each `code.patch` move has exactly one target, of a type that its `op` accepts.
- `insert` and `replace` have `content`, and `delete` has no `content`.
- Each annotation `id` is unique.
- Each step `id` obeys the rules of [Named steps](#named-steps).
- Only a `code.patch` move has `rebind`.
- A `rebind` entry for an annotation of the document is not empty.

A spotlight with no `from_line` and no `ids` also passes the schema. Tiza loads it, and `tiza check` gives an error for it.

## Status bar

| Status bar | Meaning |
|---|---|
| `↪ FILE:A-B` | A `code.link` move shows `FILE` with the window `A-B`. |
| `↪ FILE` | A `code.link` move shows `FILE` with no window. |
| `step N/M · TITLE` | The step on screen has a title. |
| `step N/M · ID` | The step on screen has an id and no title. |

The status bar cuts a long title to 32 columns. For example:

```
↪ src/queue.rs:10-36
step 3/7 · Shared borrow
```

## Messages

| Message | When |
|---|---|
| `cannot paste while moves are applied (step N): go back to step 0 first` | You paste at a step where a move is applied. |
| `cannot show another file's code while moves are applied (step N): go back to step 0 first` | `:ref` or `:link` names a file with other code while a move is applied. |
| `FILE is shown by a code.link at step N, with annotations of its own: delete that link first to reference it` | `:ref` or `:link` names a file that a `code.link` move shows. |
| `tiza: step N does not exist (the document has steps 0 to M); showing step M` | `tiza render --step N` names a step after the last step, for an export that does not record the steps. |
| `` tiza: no Tiza instance is listening on PATH (open `tiza <file>` or pass --socket/--session) `` | `tiza next`, `tiza prev` or `tiza goto` finds no instance. |
| `no step is named "ID" (named steps: IDS)` | A goto command names an unknown id. |
| `no step is named "ID": the document has no named steps` | A goto command names an id, and the document has no named steps. |
| `step N is beyond the last step (M)` | A goto command names a step number after the last step. |
| `step "ID" names step N, beyond the last step (M)` | A goto command names an id whose entry is after the last step. |
| `steps: entry «ID» names step N, beyond the last step (M); kept, not used` | An entry names a step after the last step. |
| `` steps: entry «ID» (step N) has unknown key `KEY`; kept in the file, not used (known: step, id, title, auto_advance_ms) `` | An entry has a key that Tiza does not know. |
| `steps: two entries name step N (one entry per step)` | Two entries name the same step. |
| `` steps: the entry of step N has an empty `id` (leave it out instead) `` | An `id` is empty. |
| `steps: id "ID" is used twice (ids must be unique)` | Two entries have the same `id`. |
| `` steps: id "ID" (step N) has blanks; use one word (`shared-borrow`) `` | An `id` has more than one word. |
| `steps: id "ID" (step N) is all digits; a number means a step number, so ids need a letter` | An `id` is all digits. |
| `` steps: id "ID" (step N) starts with `-`, which `tiza goto` would read as an option; start it with a letter or a digit `` | An `id` starts with `-`. |
| `` steps: `auto_advance_ms` of step N is 0; give at least 1 ms or leave it out `` | An `auto_advance_ms` is 0. |
| `WHAT: step N is past the last step a document can have (999)` | An annotation, a move or an entry names a step after 999. |

The message `no Tiza instance is listening on …` ends with the error of the system. `tiza goto` writes `tiza: ` before each goto message. It ends a message about an unknown id with ` (not_found)`. It ends a message about a step after the last step with ` (invalid_argument)`. For example:

```
tiza: no step is named "x" (named steps: a) (not_found)
```

The other messages of `tiza` are in [Messages](cli.md#messages).

## Special cases

### A paste while a move is applied

Tiza refuses a paste while a move is applied. Go back to step 0 first. The message is `cannot paste while moves are applied (step N): go back to step 0 first`.

At step 0, a paste applies the moves again on the pasted code. Tiza anchors each annotation again on the code at its own step. Thus an arrow at step 3 stays on a line that a move typed.

### Follow mode with moves

A save in the editor waits while a `code.patch` is applied. A patch is applied when its `at_step` is the current step or before it. The change loads at the step before the first patch, or at an earlier step. Thus a document without patches never waits. See [Moves in Follow the editor](../guides/follow-the-editor.md#moves).

## How it works

A document depends only on its step. Tiza applies each move up to the current step, in step order, on the code of step 0. Going back applies fewer moves. The document and `:embed` always keep the code of step 0.

A `code.patch` move gives the new place of each line. Tiza moves the annotations with the lines. Tiza does not search for text.
