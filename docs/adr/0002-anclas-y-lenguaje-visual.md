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
  - **Highlights and marks** tint the span background. Rust marks (`move ━▶`, `borrow ╌▶`, `&mut ═▶`, `drop ╳`, `lifetime ┆`) have a fixed color and glyph: they are told apart by shape, not only by color.
  - **Notes and legends** go in the right margin when ≥ 14 cells remain; otherwise in a virtual row below the line (`┆ note`, `· legend`).
- **Steps** are an integer per annotation; annotations with `step <= current step` are shown.
- A **spotlight** is an annotation that draws nothing: while active (`step..=until`) it dims the code outside its lines and the annotations outside its `ids`. It allows "look here" steps without adding strokes.

## Consequences

- Virtual rows change the row↔line mapping; `Layout::row_of_line` is the single source of truth for scrolling and the cursor.
- Arrow routing does not avoid text: it crosses box rails with `─`. Enough for the demo; fine routing is a follow-up (t-022).
- We avoid emojis in the default themes (ambiguous width).
