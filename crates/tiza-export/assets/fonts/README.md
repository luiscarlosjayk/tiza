# Embedded fonts

Compiled into the binary (`fonts.rs`) and used only by `tiza render --format svg|png`, `:export shot.svg|png` and the MCP `export_image`, so an exported image looks the same on every machine. The terminal uses the user's font.

| Files | Family | License | Used for |
|---|---|---|---|
| `JetBrainsMono-{Regular,Bold,Italic,BoldItalic}.ttf` | JetBrains Mono 2.304 | [OFL 1.1](LICENSE-JetBrainsMono-OFL.txt) | code, line numbers, box drawing, arrows, titles — and, by default, notes, labels and legends too |
| `TizaSymbols.ttf` | DejaVu Sans Mono 2.37, subset to U+2000–2BFF and U+1F300–1FAFF, renamed | [DejaVu license](LICENSE-TizaSymbols-DejaVu.txt) (Bitstream Vera + public domain) | fallback for symbols JetBrains Mono lacks (★ ✎ ⇄ …) |
| `TizaSerif-{Regular,Bold,Italic,BoldItalic}.ttf` | [Libron](https://github.com/nicoverbruggen/libron) 0.31, subset to Latin (U+0020–007E, U+00A0–024F, U+2000–206F, U+20AC, U+2122, U+2190–21FF, U+2026), renamed | [OFL 1.1](LICENSE-TizaSerif-Libron-OFL.txt) | the brand serif for notes, labels and legends, **opt-in** with `--brand-font` / `brand_font: true` (t-068) |

Provenance and the subset commands:

- Symbols: `pyftsubset DejaVuSansMono.ttf --unicodes="U+2000-2BFF,U+1F300-1FAFF" --no-hinting`, then the family renamed to *Tiza Symbols* with fontTools.
- Serif: `Libron.zip` of release v0.31 (the repo holds only FontForge sources), `pyftsubset Libron-Regular.ttf --unicodes="U+0020-007E,U+00A0-024F,U+2000-206F,U+20AC,U+2122,U+2190-21FF,U+2026" --no-hinting` for each of the four faces, then the family renamed with fontTools.

**Tiza Serif is Libron, renamed.** The OFL declares "Libron" a Reserved Font Name, so a subset (a modified version) must not be distributed under that name: the `name` table says *Tiza Serif* (`TizaSerif-*`), the copyright and license text are kept unchanged (the license file explains the rename), and the original is at github.com/nicoverbruggen/libron. Exports use it only for prose cells (`CellStyle.prose`: margin notes, labels, legends, under-line labels) and only when asked: by default everything is set in JetBrains Mono, as in the terminal, because at 1080p the monospace reads better (more air between letters, even stroke); with `--brand-font` the serif gets 1.08× the size and 0.035 em of letter-spacing. In the terminal the user's font decides; JetBrains Mono (or any humanist mono) pairs well with the serif in the exported stills.

Emoji render as monochrome outlines when the fallback has them and are otherwise left blank: pass `--system-fonts` to also use the fonts installed on the machine.
