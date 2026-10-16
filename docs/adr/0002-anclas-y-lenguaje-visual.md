# 0002 · Anchors by (line, grapheme) and visual language

**Status:** accepted (2026-10-15), to be revisited after the first video

## Context

Annotations must survive scrolling, terminal resizing and theme changes. Terminals and `unicode-width` do not always agree on the width of emojis; tabs depend on configuration. In a ~75-column terminal there is no right margin.

## Decision

- The **model** anchors in text coordinates: absolute line (1-based, preserved when opening `file.rs:12-48`) and **grapheme offset** (0-based, `unicode-segmentation`). The conversion to cells happens only when rendering (`unicode-width`, tabs at 4).
- For agents and commands, anchors are given by **text** (`find`, with optional line and occurrence) and resolved to graphemes on creation; the original `find` is saved in the `.codedraw` for future re-anchoring.
- Visual language of v0:
  - **Boxes** truly surround the range: unnumbered virtual rows above and below (`╭─ label ──╮` … `╰──╯`) and `│` rails on both sides. Boxes nest.
  - **Arrows between lines** run along a lane to the left of the code: `╭──` leaves the source line, `╰──▶` enters the target line; the longest ones take the farthest lane.
  - **Arrows on the same line** use a virtual row below (`╰───▶`).
  - **Arrow styles** are per annotation, like color: `stroke` (solid `─│`, dashed `╌╎`, dotted `┄┆`, thick `━┃`, double `═║`), `route` (rounded `╭╰` or sharp `┌└`; thick and double have no rounded glyphs and always use square corners), and `heads` at both ends (none, arrow `▶`, dot `●`, bar `┤`, open `>`). Crossings pick the junction glyph from the two strokes (`┼ ┿ ╂ ╋ ╪ ╫ ╬`). A true curve does not exist in a cell grid, so Figma's "curved" maps to rounded corners and "elbow" to sharp.
  - **Highlights and marks** tint the span's background. Marks are language-agnostic: a built-in set of kinds (`info`, `warn`, `error`, `ok`, `question`, `important`, `read`, `write`, `move`, `new`, `drop`, `scope`, `flow`, `todo`), each with a fixed glyph + legend + color so they are told apart by shape, not only color. Rust names stay as aliases. Authors extend them in two ways: a one-off inline `style` on an annotation, or reusable named `styles` in the document (`:style`, `define_style`). Resolution: inline > document > built-in. Emoji are allowed in user styles (width is measured per cell); built-ins avoid them.
  - **Notes and legends** go in the right margin when ≥ 14 cells remain; otherwise in a virtual row below the line (`┆ note`, `· legend`).
  - **Callouts** are *blocks*, not labels: a bordered paragraph (`╭─ title ─╮ │ text │ ╰─╯`, same glyphs as boxes) that explains a range of lines. In `margin` placement it is drawn in the right margin aligned with `from_line` and *reserves* its rows, so the labels of the lines it covers are drawn beside it when ≥ 14 cells remain to its right, or pushed to the first free row after it otherwise; the labels below are never displaced. In `below` placement it inserts its own virtual rows after `to_line`, clamped to the code column; a box extends to enclose them only when the box contains the callout's whole range, otherwise the rows go after that box's bottom border (box by box when nested), so a callout that sums up several functions is not read as part of the last one. `auto` picks `margin` only when the margin can hold the block and a label beside it. The distinction matters because one paragraph in the shared label column used to push every label under it out of place.
- **Steps** are an integer per annotation; annotations with `step <= current step` are shown.
- A **spotlight** is an annotation that draws nothing: while active (`step..=until`) it dims the code outside its lines and the annotations outside its `ids`. It allows "look here" steps without adding strokes.

## Consequences

- Virtual rows change the row↔line mapping; `Layout::row_of_line` is the single source of truth for scrolling and the cursor.
- Arrow routing does not avoid text: it crosses box rails with `─`. Enough for the demo; fine routing is a follow-up (t-022).
- We avoid emojis in the default themes (ambiguous width).
