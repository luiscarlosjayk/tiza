# Layout repros of the six points Luis decided (1A-6A), at `--step 1`

- `p1.tiza` (`p1b.tiza` without the note): a margin callout that falls back to a label on a line with a note goes under the line in one row, cut with «…» at 100 columns (a whole block at 61).
- `p2b.tiza` (`p2.tiza`, the second callout wider): two margin callouts on one line drawn over each other at 100 columns; `check` says nothing.
- `p3.tiza`: a callout block that fits leaves no room beside it, so the note on its line is not drawn at 100 columns; `p3b.tiza` (two more lines) draws that note beside L6 instead.
- `p4.tiza`: a long label under a line inside a box crosses the box's right rail (100 and 61 columns). `p5.tiza`: an empty `label` leaves a row with a lone `·` (`p5b.tiza`: `null`, for contrast; `p5c.tiza`: a mark and a box with an empty label). `p6.tiza` with `four.env`: a margin callout's block takes the rows of the next lines that have a note (not drawn at 100, beside L2 from 116).

From the first QA round (QA's own documents, as they were):

- `p1b.tiza` at 62–100 and `p6.tiza` at 81–97: a margin callout alone on its line with no room for its block was a `┆` label in the margin, cut with «…».
- `three-on-one-line.tiza`: three margin callouts on L2 (two side by side at 140, three at 180). `consecutive-lines.tiza`: callouts on L2, L3 and L4; at 130 the one of L4 was drawn beside L6–L8. `later-steps.tiza`: callouts and notes that arrive at later steps (at step 3, 100 columns, the second callout of L2 sat beside L5–L7).
- `blank-spaces.tiza` and `blank-null.tiza`: the same document with `"  "` and with no label on a highlight, a mark, an arrow, a box and two callout titles.
