# Changelog

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/).

## [Unreleased]

### Changed
- All user-facing text (TUI, CLI, MCP, docs, examples) is now in English. Commands, JSON fields and the Spanish color aliases (`azul`, `verde`…) are unchanged; `:theme` keeps accepting `oscuro`/`claro`.

### Added
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
