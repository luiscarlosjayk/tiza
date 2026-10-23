# Layout repros of the six points Luis decided (1A-6A), at `--step 1`

- `p1.tiza` (`p1b.tiza` without the note): a margin callout that falls back to a label on a line with a note goes under the line in one row, cut with «…» at 100 columns (a whole block at 61).
- `p2b.tiza` (`p2.tiza`, the second callout wider): two margin callouts on one line drawn over each other at 100 columns; `check` says nothing.
- `p3.tiza`: a callout block that fits leaves no room beside it, so the note on its line is not drawn at 100 columns; `p3b.tiza` (two more lines) draws that note beside L6 instead.
- `p4.tiza`: a long label under a line inside a box crosses the box's right rail (100 and 61 columns). `p5.tiza`: an empty `label` leaves a row with a lone `·` (`p5b.tiza`: `null`, for contrast; `p5c.tiza`: a mark and a box with an empty label). `p6.tiza` with `four.env`: a margin callout's block takes the rows of the next lines that have a note (not drawn at 100, beside L2 from 116).

From the first QA round (QA's own documents, as they were):

- `p1b.tiza` at 62–100 and `p6.tiza` at 81–97: a margin callout alone on its line with no room for its block was a `┆` label in the margin, cut with «…».
- `three-on-one-line.tiza`: three margin callouts on L2 (two side by side at 140, three at 180). `consecutive-lines.tiza`: callouts on L2, L3 and L4; at 130 the one of L4 was drawn beside L6–L8. `later-steps.tiza`: callouts and notes that arrive at later steps (at step 3, 100 columns, the second callout of L2 sat beside L5–L7).
- `blank-spaces.tiza` and `blank-null.tiza`: the same document with `"  "` and with no label on a highlight, a mark, an arrow, a box and two callout titles.

From the second rework (a row under a line beside a margin block, found by t-216):

- `under-beside-block.tiza`: the callout of `four.env` on L1 and t-216's long note on L2. At 99 columns the note goes under L2 and the block covers that row; it ran under the block, cut with no «…» and no warning.
- `arrow-beside-block.tiza`: the same callout, with the labels of an arrow that ends on L2 and of an arrow along L2, both under L2 beside the block.

From the third rework (QA 1's final QA of the margin callouts):

- `narrow-block.tiza`: a margin callout on L2 of three short lines (`A=1`), with no rows for its block in the margin, so it is a block under its line. The code column is 10 cells: the text wrapped at 8 and the block held 6, so each row lost its end, with no «…» and no warning, at every width. `narrow-block-boxed.tiza`: the same inside a box (its block must stay inside the box rail).
- `under-rows.tiza`: a long note on L1 continued beside L2, whose two notes are under it, and wrote over the first of them at 70 columns, with no warning. `under-rows-long.tiza`: the same with a longer first note on L2.
- `block-before-box.tiza`: callouts on L1 and L3 and a box on L4-L6. At 60 columns the block of L3 started on the box's top row, inside the box, beside L4 and L5.
