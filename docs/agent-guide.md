# Agent guide

This page is for an agent (Claude Code, Zed, any MCP client) that is going to draw on the code a presenter has open in CodeDraw.

## Mental model

- The **TUI is the sole owner of the document**. You talk to it; you do not edit the `.codedraw` by hand while it is open.
- A document has **code** (numbered lines, absolute numbering even when a range is opened) and **annotations**. Each annotation has a short, stable **id** (`a1`, `a2`… or the one you pass) and a **step** (1-based). Annotations with `step <= current step` are shown.
- **Anchors** are positions in the text: `{line, col}` or, better, `{find: "exact text", line?: N, occurrence?: K}`. `find` searches for the text on the given line (if you pass one) and then in the whole document. If it does not exist, the tool fails with `text_not_found`: do not guess columns.

## Recommended flow

1. `status` — confirm there is an instance and look at `cols`/`rows` and `warnings` (`small terminal …: labels go under the lines` → keep labels short; `code clipped (N cols)` → the presenter should widen the terminal or open a narrower range).
2. `get_code` — read the code with line numbers. Pick exact texts.
3. Annotate in the order of the explanation, assigning an increasing `step`:
   - `add_mark` for semantic marks. Built-in, language-agnostic kinds: `info`, `warn`, `error`, `ok`, `question`, `important`, `read`, `write`, `move`, `new`, `drop`, `scope`, `flow`, `todo` (Rust habits work as aliases: `borrow`→`read`, `borrow_mut`→`write`, `lifetime`→`scope`). Each shows glyph + legend; `label` is appended. For something the author will reuse, `define_style` once (`{name, glyph, legend, color}`) and then use `name` as `kind`; for a one-off, pass `style: {glyph, legend, color}` inline. Emoji are allowed as glyphs. `list_kinds` shows what exists.
   - `add_arrow` for relationships (from where a value is born to where it is consumed). Style is the author's: `stroke` solid | dashed | dotted | thick | double, `route` rounded | sharp (a cell grid has no curves: rounded corners are the "curved" arrow), `head_start` / `head_end` none | arrow | dot | bar | open (default none → arrow; none + none is a plain connector). A convention that reads well: solid for a move, dashed for a shared borrow, thick for a mutable borrow. `update_annotation` changes them later (`stroke`, `route`, `head_start`, `head_end`); crossings pick the right junction (┼ ╂ ╋ ╫ …) automatically.
   - `add_box` to delimit a function or a block.
   - `add_note` for a short sentence (one line, in the margin); `add_callout` for a paragraph of two or three sentences about a range of lines: it is a bordered block with an optional `title`, it does not displace the other labels, and `placement` is `margin` (default), `below` (own rows under `to_line`; inside a box only when the box contains the callout's whole range, otherwise after the box's bottom border) or `auto`. Use `add_box` when you want to *delimit* code, `add_note` to tag a line, `add_callout` to *explain*; `highlight` for a fragment without semantics.
   - `add_spotlight` for a "look here" step: dims the code outside `from_line..to_line` and/or the annotations outside `ids` (and focuses the code of those annotations). It draws nothing; it directs attention. Active from `step` to `until`.
4. `set_step 0` leaves only the code (a good starting point for the take); then `next_step` as the presenter advances (or leave it in their hands: `n`/`p`).
5. `render_text` to see exactly what the audience sees and fix things. `export_image` with a `.png` or `.svg` path saves the scene as an image (fonts embedded; `step`, `width`, `theme`, `scale` optional) for thumbnails and docs.

## Example (ownership)

```json
{"name":"add_mark","arguments":{"kind":"move","at":{"line":3,"find":"s"},"label":"s moves into takes","step":1}}
{"name":"add_arrow","arguments":{"from":{"line":2,"find":"s"},"to":{"line":15,"find":"s: String"},"label":"move","step":1,"color":"orange"}}
{"name":"add_arrow","arguments":{"from":{"line":7,"find":"&t"},"to":{"line":19,"find":"s: &String"},"label":"shared borrow","stroke":"dashed","step":3}}
{"name":"update_annotation","arguments":{"id":"a2","stroke":"thick","head_end":"bar"}}
{"name":"add_note","arguments":{"line":4,"text":"s no longer owns it: using it here does not compile","step":2}}
{"name":"add_box","arguments":{"from_line":15,"to_line":17,"label":"takes owns s and frees it at the end","step":2}}
{"name":"add_callout","arguments":{"from_line":15,"to_line":25,"title":"three ways to pass a value","text":"takes(s) moves the String. len(&s) borrows it read-only. push(&mut s) borrows it mutably.","placement":"auto","step":3,"color":"cyan"}}
```

## Color

Every annotation accepts `color`: `orange`, `blue`, `green`, `purple`, `pink`, `cyan`, `yellow`, `red`, `gray` (Spanish aliases such as `azul`, `verde`… also work) or `#rrggbb`. Use it to tell a story per value: everything that happens to `s` in one color, everything about `t` in another. Arrows without a color rotate through an automatic palette; boxes, notes and highlights without a color use the theme's. `update_annotation` with `color: ""` returns to automatic.

## Fixing a misplaced annotation

`update_annotation` moves an annotation without recreating it, so it keeps its id, kind, style, label, color and place in the list: `at` for highlights, marks and notes (`{"id": "a1", "at": {"line": 15, "find": "s: String"}}`), `from`/`to` for arrows, `from_line`/`to_line` for boxes, callouts, spotlights and notes. A field that does not apply to the annotation's type fails with `invalid_argument` (e.g. `from_line` on a mark), and unknown fields are rejected, so a call that returns a new revision did change something. Over the socket the same goes under `move`: `annotation.update {"id": "a1", "move": {"at": {...}}}`.

## Between takes

`clear_annotations` empties the canvas at once (or only one step with `step`). The presenter can undo it with `u`, so it is not destructive live.

## Idempotency

Pass your own `id` (`"id": "move-s"`) so that a retry does not duplicate the annotation: the second attempt fails with `duplicate_id` and you can move on.

## Without MCP: socket and CLI

Same contract, one JSON request per line over the Unix socket:

```sh
printf '%s\n' '{"id":1,"method":"annotation.add","params":{"type":"note","line":4,"text":"hello"}}' | nc -U "$TMPDIR/codedraw/default.sock"
codedraw api annotation.add '{"type":"note","line":4,"text":"hello"}'
codedraw api api.methods
```

Response: `{"id":1,"result":{...}}` or `{"id":1,"error":{"code":"text_not_found","message":"..."}}`. Codes: `out_of_range`, `not_found`, `duplicate_id`, `text_not_found`, `invalid_argument`, `invalid_json`, `unavailable`.
