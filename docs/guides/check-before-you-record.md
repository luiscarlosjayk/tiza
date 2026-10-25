# Check before you record

Use this checklist before you record a video or a GIF. `tiza check` finds the problems of a document before you record it.

Tiza is a terminal tool that draws annotations on code. To install it, see [Install](install.md).

## Checklist

1. Run `tiza check demo.tiza`. It loads the document as the TUI does, and lists every problem at once.
2. Fix the errors. When there is an error, `tiza check` exits with 1.
3. Read the warnings. Add `--strict` to make a warning also exit with 1.
4. Pick the width. The last line of the report gives the narrowest width where no label is cut. Use that width or more.
5. Use demo values for tokens, passwords and credentials. The audience sees what is in the file.
6. Set up the terminal. See [Terminal and font](#terminal-and-font).
7. Before you record, go through the steps in normal mode. Look for a `!` badge on the status bar. See [Badges on the status bar](#badges-on-the-status-bar).

## What check reports

`tiza check` does not change any file. It checks the labels at the width that you record at: 100 columns, or `--width N`.

The table gives the usual problems. Each finding in the JSON output also has a `code`.

| Problem | Level |
|---|---|
| The document does not follow the `.tiza` format. | Error |
| An anchor, a marker or a move target does not match at its step. | Error |
| A move target matches more than one time at its step. | Error |
| A linked file cannot be read. | Error |
| An id is unknown. | Error |
| Two annotations have the same id. | Error |
| A move hides an annotation. | Warning |
| An annotation is outside the `code.link` window of its step. | Warning |
| A later move pushes an annotation out of the `code.link` window. | Warning |
| An annotation belongs to a file that its own step does not show. | Warning |
| A label is cut with `…` at the width that you record at. | Warning |
| A step changes nothing. | Warning |
| A marker (`find_start` or `find_end`) matches more than one time. | Warning |

- `tiza check` reports a file that its own step does not show also when the file shows at other steps. Tiza never draws the annotation at its own step.
- For a marker that matches more than one time, the warning gives the lines of each match. The range takes the first match.

Each finding is one line: `error: ID, step N, LINE: MESSAGE` or `warning: ID, step N, LINE: MESSAGE`. `ID` and `LINE` show only when they apply. `LINE` is `L` and the line number. For example:

```
error: x1, step 2, L99: line 99 does not exist (the document spans 1 to 25)
warning: step 7: nothing new on this step: the presenter presses a key and nothing changes
```

### The width line

The report ends with the narrowest width where no label is cut. For example, for `examples/ownership.rs.tiza`, the line is:

```
no label is clipped at 91 or wider (`tiza render --width 91`)
```

At a narrow width, notes go under their lines. Then a narrower width can be clean again. When you ask for such a width, the report gives that range too. For example, with `--width 80`:

```
no label is clipped at 91 or wider; 70-84 are clean too (notes go under the lines below 85 columns)
```

## Badges on the status bar

The status bar shows the terminal size as `COLS×ROWS`. It shows a `!` badge only when the step on screen cuts or hides a label, a note or a code line.

| Status bar | Meaning |
|---|---|
| `! N labels cut` | `N` labels or notes are cut with `…`, or are not drawn. |
| `! code needs N cols` | A code line is cut. |

- In `! code needs N cols`, `N` is the terminal width that shows every code line of the step.
- When the status bar has no space, `! N labels cut` gets shorter and shows only `!`.
- The `status` reply of the API lists the details in `warnings`, for example `2 labels cut at 40 columns: a4, a3`.

## Terminal and font

- Use a full-screen terminal, or one that fills half of the screen next to Zed.
- Use a font of 18 to 22 px.
- Keep a snippet to 60 columns or fewer.
- The exports set all the text in JetBrains Mono: the code, the notes, the labels and the legends. Use JetBrains Mono in the terminal to make it match the exports. Any other monospace font also works.

## Narrow terminals

- Below about 100 columns, Tiza draws notes under their lines, not in the margin.
- The width depends on the document. `tiza check` gives it: see [The width line](#the-width-line).
- Then the label of an arrow goes on the row of the arrowhead, after the code, when it fits. Otherwise, it goes on the row below, after the glyph of the arrow: `─▶ move`. Thus the label reads as part of the arrow, not as a separate comment.
- Tiza draws a callout with `placement: auto` under its lines.
- By default, Tiza keeps one blank column at each side. `:pad V H` changes the padding. `:pad 0 0` removes it. `:pad default` sets it back.

## JSON output

`tiza check --json` gives the same findings to a script.

| Field | Meaning |
|---|---|
| `ok` | `true` when the check found no error. |
| `errors` | The number of errors. |
| `warnings` | The number of warnings. |
| `width` | The width of the check. |
| `max_step` | The last step of the document. |
| `min_clean_width` | The narrowest width where no label is cut. |
| `findings` | The list of findings. |

Each finding has `level`, `code` and `message`. It also has `id`, `step` and `line` when they apply.

The `code` of a finding is one of these:

| Code | Level | What it reports |
|---|---|---|
| `schema` | Error | The document does not follow the `.tiza` format. |
| `code` | Error | Tiza cannot read the code of the document. |
| `bounds` | Error | A number is out of range, for example a step past the limit. |
| `link` | Error | A linked file cannot be read. |
| `duplicate_id` | Error | Two annotations have the same id. |
| `steps` | Error or warning | An entry of `steps` is not valid, or Tiza does not use it. |
| `move` | Error | A move is not valid, or its target does not match at its step. |
| `rebind` | Error | The `rebind` of a move is not valid. |
| `stale` | Error | In a live instance, a move does not match the code after a reload or a paste. |
| `unknown_id` | Error or warning | An annotation or a `rebind` names something that does not exist. |
| `unknown_file` | Error | An annotation names a file that no `code.link` shows. |
| `anchor` | Error | An anchor does not match at its step. |
| `marker` | Error or warning | A marker does not match, or it matches more than one time. |
| `unanchored` | Warning | A move hides an annotation. |
| `outside_window` | Warning | An annotation is outside the `code.link` window of its step. |
| `not_shown` | Warning | An annotation belongs to a file that its own step does not show. |
| `empty_step` | Warning | A step changes nothing. |
| `clipped_label` | Warning | A label is cut at the width of the check. |

## Check a live instance

A live instance gives the same report for the document that it holds. The report includes the changes that are not saved. Use `tiza api check '{"width": 108}'`, or the MCP tool `check`.

## How it works

Both themes use explicit RGB colors for 1080p video. `scripts/legibility.py` runs in CI. It measures each text color after a compression like the one on YouTube. Each text color keeps a contrast of 4.5:1 or more against the background.
