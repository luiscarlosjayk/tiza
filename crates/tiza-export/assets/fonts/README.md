# Embedded fonts

Compiled into the binary (`fonts.rs`) and used only by `tiza render --format svg|png`, `:export shot.svg|png` and the MCP `export_image`, so an exported image looks the same on every machine. The terminal uses the user's font.

| Files | Family | License | Used for |
|---|---|---|---|
| `JetBrainsMono-{Regular,Bold,Italic,BoldItalic}.ttf` | JetBrains Mono 2.304, the four faces complete (not subset) | [OFL 1.1](LICENSE-JetBrainsMono-OFL.txt) | everything: code, line numbers, box drawing, arrows, titles, notes, labels and legends |
| `TizaSymbols.ttf` | DejaVu Sans Mono 2.37, subset to U+2000–2BFF and U+1F300–1FAFF, renamed | [DejaVu license](LICENSE-TizaSymbols-DejaVu.txt) (Bitstream Vera + public domain) | fallback for symbols JetBrains Mono lacks (★ ✎ ⇄ …) |

Provenance and the subset command of the symbols font: `pyftsubset DejaVuSansMono.ttf --unicodes="U+2000-2BFF,U+1F300-1FAFF" --no-hinting`, then the family renamed to *Tiza Symbols* with fontTools.

The brand serif (*Tiza Serif*, a renamed Libron subset) used to be embedded here for the opt-in `--brand-font`; it left the binary on 2026-10-18 (t-134) and now lives in [`docs/brand/fonts/`](../../../../docs/brand/fonts/README.md) as brand material (wordmark, thumbnails: t-069), with its OFL license and the Reserved Font Name note.

Emoji render as monochrome outlines when the fallback has them and are otherwise left blank: pass `--system-fonts` to also use the fonts installed on the machine.
