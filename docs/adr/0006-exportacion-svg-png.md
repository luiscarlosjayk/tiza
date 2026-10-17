# ADR 0006: SVG and PNG export with embedded fonts

> Renamed: the project became **Tiza** on 2026-10-17 ([ADR 0008](0008-renombre-a-tiza.md)). Where this record says CodeDraw, read Tiza; `codedraw` → `tiza`, `.codedraw` → `.tiza`, `CODEDRAW_*` → `TIZA_*`.

- Status: accepted (2026-10-16)
- Board: t-050 (fonts for notes: t-068)

## Context

The same drawing that is shown on video is wanted as a still image: YouTube thumbnails, posts, docs, the README. It has to look identical on every machine and must not depend on the terminal or on the fonts installed there. An agent should be able to produce it too.

## Decision

- A new crate, `codedraw-export`, turns the rendered `Canvas` (cells with fg/bg/bold/italic/dim) into an **SVG**: one `<rect>` per background run and one `<text>` per run of equal style, on a monospace grid of 0.6 × 1.32 em with `textLength` + `lengthAdjust="spacing"` so every run lands exactly on its cells. The background is the theme's, with an optional corner radius and a margin in cells.
- **PNG = that SVG rasterized** with `resvg` (`usvg` + `tiny-skia` + `fontdb`, pure Rust, no system libraries), at a device `scale` (default 2).
- **Fonts travel inside the binary**: JetBrains Mono (Regular, Bold, Italic, Bold Italic; SIL OFL 1.1) plus *CodeDraw Symbols*, a subset of DejaVu Sans Mono (Bitstream Vera license, renamed as the license requires) for the symbols JetBrains Mono lacks (★ ✎ ⇄ …). They are loaded into the rasterizer and, by default, embedded in the SVG as `@font-face` data URIs (~1.2 MB; `--no-embed-fonts` for a small SVG that relies on the viewer's fonts). System fonts are used only when asked (`--system-fonts`), as a fallback for emoji and CJK, because the default output must be reproducible.
- The CLI picks the format from the extension (`render --out x.png|x.svg`), and **without a file it exports the live instance** (`scene.get` now sends the code embedded so a client in another directory can render it). The MCP exposes the same as `export_image`.
- The terminal cell ratio is not reproduced: the export uses the font's own metrics (ascender 1020 + descender 300 per 1000 upem), which is what keeps the box-drawing glyphs connected.

## Consequences

- The binary grows by ~1.2 MB of fonts; `codedraw-export` is the only crate that links `resvg`, so the TUI stays as it was.
- Emoji in user styles render only with `--system-fonts` (and depend on the machine). Documented limitation.
- Libron (the brand font) is not used yet: proportional text would break the cell grid. t-068 will evaluate it for notes and callouts with per-run `textLength`.
- Licenses: `tiny-skia` is BSD-3-Clause, now allowed in `deny.toml`. Font licenses ship next to the fonts in `crates/codedraw-export/assets/fonts/`.

## Amendment (t-068): brand serif for prose

Exports set prose cells — margin notes, labels, mark legends, under-line labels — in **CodeDraw Serif**, a Latin subset of [Libron](https://github.com/nicoverbruggen/libron) (OFL 1.1; renamed because *Libron* is a Reserved Font Name), at 1.08× the grid font size so its x-height matches JetBrains Mono. Code, line numbers, box borders and box/callout titles stay monospace, because they must sit on the cell grid. Prose runs are emitted without `textLength` (natural width; the serif is narrower than the grid and the margin has room to the right), and a mark's leading glyphs (`━▶`, `╳`) are split into their own monospace run, because resvg sets a whole run in one font when some family has every glyph. The layout still wraps and clips labels by cell count (what the terminal shows), so a clipped label stays clipped in the export even when the serif would have had room. `--mono-labels` (`SvgOptions.brand_font = false`) restores the all-monospace export; the terminal never sees the flag.
