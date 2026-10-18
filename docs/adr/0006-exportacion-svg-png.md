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
- Licenses: `tiny-skia` is BSD-3-Clause, now allowed in `deny.toml`. Font licenses ship next to the fonts in `crates/codedraw-export/assets/fonts/`.

## Amendment (t-068): brand serif for prose, opt-in — removed from the binary (t-134)

> **Superseded on 2026-10-18 (t-134, Luis's decision): the serif is no longer in the binary.** Since the monospace became the default (below), Tiza Serif only served the opt-in `--brand-font`, and it cost ~0.34 MB of embedded fonts plus a second rendering path (prose runs without `textLength`, letter-spacing, class `p`) that every visual change had to re-check. Reasons: binary size, and one rendering path. Every export (SVG, PNG, GIF, video) is now set in JetBrains Mono — the four faces kept complete, not subset — plus the symbols fallback; `SvgOptions.brand_font`, the `.p` style and the serif `@font-face` blocks are gone (the default SVG drops ~0.46 MB of base64), and the default output is otherwise byte-identical to what it was. `--brand-font` and the hidden `--mono-labels` are accepted for one prerelease as no-ops that print one notice on stderr, then go away; `export_image` no longer documents `brand_font` (an old client that still sends it is ignored, since the parameters do not reject unknown fields). The mark-glyph split (`━▶`, `╳`, `✎▶` in their own run) stays: the glyphs from the fallback font are not exactly one cell wide, and in one `textLength` run the difference would shift the words. The Libron subset and its OFL license moved to [`docs/brand/fonts/`](../brand/fonts/README.md) as brand material (README wordmark, thumbnails, video titles: t-069), outside every crate. The text below is kept as the record of what t-068 shipped.

Exports can set prose cells — margin notes, labels, mark legends, under-line labels — in **CodeDraw Serif** (the files are `TizaSerif-*` since the rename, ADR 0008), a Latin subset of [Libron](https://github.com/nicoverbruggen/libron) (OFL 1.1; renamed because *Libron* is a Reserved Font Name), at 1.08× the grid font size so its x-height matches JetBrains Mono and with 0.035 em of letter-spacing (Libron is tight at this size; without it the italics clump). Code, line numbers, box borders and box/callout titles stay monospace, because they must sit on the cell grid. Prose runs are emitted without `textLength` (natural width; the serif is narrower than the grid and the margin has room to the right), and a mark's leading glyphs (`━▶`, `╳`) are split into their own monospace run, because resvg sets a whole run in one font when some family has every glyph. The layout still wraps and clips labels by cell count (what the terminal shows), so a clipped label stays clipped in the export even when the serif would have had room.

**Default (decided by Luis on 2026-10-17, after comparing the two PNGs at 100 %): the monospace.** The serif read narrower and its italics clumped; JetBrains Mono has more air between letters and an even stroke at 1080p. So `SvgOptions.brand_font` is `false` by default and the serif is opt-in: `tiza render --brand-font`, `:export … ` has no switch (it follows the default), `export_image {brand_font: true}` over MCP. `--mono-labels`, the former switch, stays as a hidden no-op. The terminal never sees any of this; the text exports are byte-identical either way.
