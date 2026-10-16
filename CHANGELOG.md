# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/).

## [Unreleased]

### Changed
- All user-facing text (TUI, CLI, MCP, docs, examples) is now in English. Commands, JSON fields and the Spanish color aliases (`azul`, `verde`…) are unchanged; `:theme` keeps accepting `oscuro`/`claro`.

### Fixed
- Labels, notes and titles are single-line (control characters become spaces); loading a document clamps `step` to its last step and rejects duplicate ids; the line-number gutter reserves a column for the cursor/selection marker so three-digit numbers keep every digit; `--padding` is taken into account by the viewport warning; `theme.set` with a non-string value is rejected; help and list overlays no longer panic on terminals under 8×4.
- `:pad 0 0` really removes the padding (it used to fall back to the default); `:pad default` restores it. SVG/PNG size no longer gains a pixel from float rounding (50 cols → 480 px).
- A callout placed below is drawn inside a box only when the box contains its whole range; a callout wider than the box (e.g. one that sums up several functions) now goes after the box's bottom border, without the box's rail or background. The ownership example's `move` mark on line 3 points at the argument `s` instead of the `s` in `takes`.

### Added
- Render snapshots (insta + ratatui TestBackend) for every annotation kind, both themes, step 0, presentation mode, selections and the small-terminal badge: `qa/tests/t-053.rs`, 32 snapshots (symbols and styles).
- JSON Schema of the `.codedraw` document and of the local API, generated from the types (`codedraw schema document|api`, `api.schema` over the socket), committed under `schema/` and checked by a test; CI validates every example and fixture against it. `-` means stdin for `codedraw -`, `codedraw render -` (code with `--lang`, or a document) and `codedraw api METHOD -`.
- Box and callout border styles: `border` (solid, dashed, dotted, thick, double), `corners` (rounded, sharp) and `fill` (boxes tinted by default, callouts not). `:box … border=double corners=sharp fill=no`, `:callout … border=…`, `:set ID border|corners|fill=…`, socket fields, MCP `add_box` / `add_callout` / `update_annotation`. A callout below the code never grows past the code column.
- README: an animated GIF of the ownership example, generated from the PNG export of each step (`docs/img/ownership.gif`).
- Property tests (proptest) in `codedraw-core`: anchors never panic on arbitrary code, the layout never overflows the width (20..=240 columns, tabs/emoji/CJK/RTL) and documents round-trip through JSON.
- Paste re-anchors annotations by the text they pointed at (same line first, then anywhere); the ones whose text is gone are dropped with a message and `u` restores them. Pasting while typing a `:` command appends to the command; an empty paste is refused.
- Help and list overlays scroll (`j`/`k`, PgUp/PgDn, `g`/`G`) and show how much is left; `:`, `?` and `L` work while a selection is active; `Tab` at step 0 explains that nothing is visible.
- Arrow styles, per annotation like color: `stroke` (solid, dashed, dotted, thick, double), `route` (rounded, sharp) and `heads` at both ends (none, arrow, dot, bar, open; none+none is a plain connector). `:arrow … stroke=dashed heads=dot,bar route=sharp`, new `:set ID key=value…` (also `e` on a selected annotation), socket fields on `annotation.add/update`, MCP `add_arrow` / `update_annotation` with `stroke`, `route`, `head_start`, `head_end`. Crossings now choose the junction from the two strokes (┼ ┿ ╂ ╋ ╪ ╫ ╬). The ownership example draws the shared borrow dashed and the mutable borrow thick.
- Viewport warnings: a ⚠ badge in the status line when the terminal is smaller than 100×30 (saying when labels go under the lines) or when the widest code line is clipped; `status` returns the same texts in `warnings`. Narrow status bars drop the theme/file/cursor segments before cutting the warning.
- `qa/`: one integration-test crate per board ticket (`qa/tests/t-XXX.rs`, fixtures in `qa/fixtures/`), with a headless `Live` instance (real `App` + socket) to drive keys, commands and the API without a terminal.
- SVG and PNG export (`codedraw render FILE --out x.svg|x.png`, `--step`, `--theme`, `--scale`, `--font-size`, `--margin`, `--radius`, `--no-embed-fonts`, `--system-fonts`). Without FILE it exports what the live instance is showing. MCP tool `export_image`. Fonts (JetBrains Mono + a symbols fallback) are embedded in the binary and in the SVG, so the image looks the same everywhere. New crate `codedraw-export`.
- `scene.get` sends the code embedded, so clients in another directory can render the live document.
- `update_annotation` (MCP) and `annotation.update` (socket, `move`) can move an annotation keeping its type, kind, style and label: `at` for highlights/marks/notes, `from`/`to` for arrows, `from_line`/`to_line` for boxes, callouts, spotlights and notes. Fields that do not apply to the type fail with `invalid_argument`; the MCP tool rejects unknown fields instead of ignoring them.
- `callout` annotation: a bordered multi-line paragraph explaining a range of lines, with optional title, `placement` (`margin`, `below`, `auto`) and `width`. `:callout A[-B] [@below|@margin|@auto] [title="…"] text`, `C` on a selection, `annotation.add` with `type: callout`, MCP `add_callout`. In the margin it reserves its rows and the neighbouring labels are drawn beside it instead of being pushed down; below, boxes extend around it. The ownership example closes with one (step 7).
- Default padding of one column at the left and right edges when the document sets none (`:pad 0 0` removes it, `:pad default` restores the default).
- `Tab`/`Shift-Tab` select an annotation in reading order; on it, `Enter` edits the label, `x` deletes, `c` changes the color, `+`/`-` move it between steps.
- Command line: `Tab` completes commands, mark kinds, annotation ids and themes; `↑`/`↓` browse the history.
- Padding around the drawing: `--padding V[,H]` when opening and `:pad V [H]` live; stored in the document (`padding: [v, h]`).
- Keyboard selection, vim style: column cursor with `h`/`l`/`w`/`b`/`e`/`0`/`$`, `v`/`V`/`i` selections, and selection actions `h` (highlight), `m` (mark, command line pre-filled), `n` (note), `b` (box), `a` (arrow in two selections), `s` (spotlight). Explicit anchors `L@S..E` and `L@C`. The annotation list moves to `L`.
- Generic, language-agnostic mark kinds (info, warn, error, ok, question, important, read, write, move, new, drop, scope, flow, todo) replacing the Rust-only set; Rust names remain as aliases. Custom styles in two modes: inline per annotation (`style: {glyph, legend, color}`, emoji allowed) and reusable document `styles` (`:style NAME …`, MCP `define_style`, `list_kinds`). `:mark 🔥 L:text legend` creates a one-off mark.
- `spotlight` annotation: a step that directs attention by dimming the code outside a range and/or the annotations outside a group (`:spot`, `:until`, `add_spotlight`, `annotation.add` with `type: spotlight`). The ownership example interleaves two.
- `code` as an object `{"file", "lines"}` with absolute paths or paths relative to the `.codedraw`; sidecars reference the source by default. `:embed` and `:link`.
- Step 0 = code only: `p` from step 1, `:step 0`, `--step 0` or `set_step 0` show the code without annotations. The ownership example starts that way.
- Clear annotations at once: `:clear [N|.]`, the `scene.clear` method and the `clear_annotations` MCP tool; reversible with `u`.
- Color per annotation (`color`: palette name or `#rrggbb`) in the model, `:color ID …`, API and MCP. Arrows without a color rotate through a palette so they can be told apart; the ownership example uses one color per value.

## [0.0.1] - 2026-10-16

First working demo.

### Added
- TUI (ratatui + crossterm): code with syntax highlighting (syntect + two-face), line cursor, scrolling, switchable dark/light theme, presentation mode, steps, undo/redo, annotation list and help.
- Annotations anchored to the text: highlights, Rust marks (move, borrow, &mut, drop, lifetime), boxes with virtual rows, arrows along a left lane and on the same line, notes in the margin or below.
- `:` command line (`:mark`, `:arrow`, `:box`, `:hl`, `:note`, `:step`, `:del`, `:label`, `:focus`, `:theme`, `:lang`, `:w`, `:q`…).
- Input: file, file with range `a.rs:12-48`, stdin, paste (bracketed paste).
- `.codedraw` format (JSON, `schema: 1`) with atomic autosave and a sidecar next to the source.
- Local API over a Unix socket (NDJSON, herdr style) and `codedraw scene|api|render` CLI.
- MCP server (`codedraw mcp`) with 16 tools, text anchors (`find`).
- `examples/ownership.rs.codedraw` example (5 steps), agent guide, Zed integration, ADRs 0001–0005.
- CI (fmt, clippy, tests on macOS and Linux, cargo-deny) and releases with binaries from tags.
