# Examples

Open them with `tiza examples/<file>.tiza --no-save` and advance with `n`. Both start at step 0 (code only). `--no-save` keeps the example untouched while you try things (every change would otherwise be written back to the file, and `git status` would show it); `:w PATH` saves your version elsewhere.

| File | What it shows | `code` mode |
|---|---|---|
| `ownership.rs.tiza` | move, borrow `&`, `&mut` and drop in 6 steps (two of them with a spotlight), with a box on each of the three functions, one color per value, and a reusable custom style `borrow_mut` defined in the document (overrides the built-in alias) | relative reference: `{"file": "ownership.rs"}` |
| `stack-impl.tiza` | `&self` vs `&mut self` in an `impl`, `Option<&T>` | reference with a subdirectory and a range: `{"file": "src/stack.rs", "lines": "7-30"}` |
| `ownership-moves.rs.tiza` | the same lesson with **moves**: step 2 types a borrow and a `len` helper into the code line by line, step 4 replaces the call with `t.len()` character by character and deletes the helper (its box and arrow become `(unanchored)`, the mark is rebound with `rebind`), step 5 sums up | embedded (the step-0 code; `moves` carry the rest) |
| `tally-tour.tiza` | **a codebase tour** over `tally/`, a four-file Rust CLI (a tiny `wc`): step 1 numbers the flow in `main.rs`, steps 2-4 link `args.rs`, `count.rs` and `report.rs`, each with its own boxes and notes, step 5 comes back to `main.rs` (lines 9-28) with a selection, a summing-up callout and a spotlight; five named steps. Built from the MCP calls of the prompt list in [docs/demos/tour-of-tally.md](../docs/demos/tour-of-tally.md); record it at `--width 130` | relative reference `{"file": "tally/src/main.rs"}` plus `code.link` moves |
| `stack-and-queue.tiza` | **two files, each with its own annotations**: step 1 boxes `push`/`pop` in `src/stack.rs`; step 2 links to `src/queue.rs:10-36` with its own box and note (a note on L4 is outside the window: kept, not drawn); step 3 selects `push_back`…`pop_front`, moves the cursor and waits 800 ms; step 4 links back to `stack.rs` — its step-1 annotations exactly as they were, plus a new note; step 5 shows `queue.rs:17-23`, a narrower window of the same view | relative reference `{"file": "src/stack.rs", "lines": "7-30"}` plus `code.link` moves (`:embed` would freeze both files under `views`) |

## The three `code` modes

```jsonc
// 1. Reference relative to the .tiza directory (what sidecars create)
"code": { "file": "src/stack.rs", "lines": "7-30" }

// 2. Absolute reference
"code": { "file": "/path/to/app/src/main.rs" }

// 3. Embedded: portable, frozen (`:embed` inside the TUI)
"code": ["fn main() {", "    println!(\"hello\");", "}"]
```

`lines` is optional and accepts `"12-48"`, `"12..48"` or `"7"`. With a reference, the document always shows the current file and the numbering is the file's real one (it starts at `lines`), so you can say "look at line 20" and match the editor.

To generate a sidecar from any source: `tiza src/main.rs:12-48` creates `src/main.rs.12-48.tiza` in reference mode.

## Custom mark styles

`ownership.rs.tiza` defines its own style and uses it like any built-in kind:

```json
"styles": { "borrow_mut": { "glyph": "✎▶", "legend": "&mut borrow", "color": "yellow" } },
"annotations": [ { "type": "mark", "kind": "borrow_mut", "span": { "line": 11, "start": 9, "end": 15 } } ]
```

A document style wins over a built-in kind or alias of the same name (`borrow_mut` is normally an alias of `write`). For a one-off, put the style inline on the annotation instead: `"kind": "test", "style": {"glyph": "🧪", "legend": "needs a test"}`. In the TUI: `:style borrow_mut glyph=✎▶ legend="&mut borrow" color=yellow`, then `:mark borrow_mut 11:&mut u`.

## Themes

`themes/catppuccin-mocha.toml` is the one shipped theme file: Catppuccin's published Mocha values on Tiza's roles, and the example to copy when writing your own (`tiza FILE --theme-file examples/themes/catppuccin-mocha.toml`, or `:theme file PATH` inside the TUI; paste the tokens into the `[theme]` table of `~/.config/tiza/config.toml` to apply them everywhere). See [Themes](https://github.com/luiscarlosjayk/tiza/blob/main/docs/reference/themes.md).
