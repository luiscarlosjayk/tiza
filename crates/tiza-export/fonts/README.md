# Embedded fonts

Used only by `tiza render --format svg|png` so the exported image looks the same on every machine.

| File | Family | License | Notes |
|---|---|---|---|
| `JetBrainsMono-{Regular,Bold,Italic,BoldItalic}.ttf` | JetBrains Mono 2.304 | [OFL 1.1](JetBrainsMono-OFL.txt) | code, labels, box drawing, arrows |
| `DejaVuSansMono-Symbols.ttf` | DejaVu Sans Mono 2.37, subset to U+2000–2BFF and U+1F300–1FAFF | [DejaVu license](DejaVu-LICENSE.txt) (Bitstream Vera + public domain) | fallback for symbols JetBrains Mono lacks (★ ✎ ⇄ …) |
| `TizaSerif-{Regular,Bold,Italic,BoldItalic}.ttf` | [Libron](https://github.com/nicoverbruggen/libron) 0.31, subset to Latin (U+0020–007E, U+00A0–024F, U+2000–206F, U+20AC, U+2122, U+2190–21FF, U+2026) | [OFL 1.1](../assets/fonts/LICENSE-TizaSerif-Libron-OFL.txt) | the brand serif for notes, labels and legends in exports (t-068) |

The subset was produced with `pyftsubset DejaVuSansMono.ttf --unicodes="U+2000-2BFF,U+1F300-1FAFF" --no-hinting`.

**Tiza Serif is Libron, renamed.** The OFL declares "Libron" a Reserved Font Name, so a subset (a modified version) must not be distributed under that name: the `name` table says *Tiza Serif* (`TizaSerif-*`), the copyright and license text are kept unchanged, and the original is at github.com/nicoverbruggen/libron (`Libron.zip` of release v0.31). Subset: `pyftsubset Libron-Regular.ttf --unicodes="U+0020-007E,U+00A0-024F,U+2000-206F,U+20AC,U+2122,U+2190-21FF,U+2026" --no-hinting`, then the family renamed with fontTools. Exports use it only for prose cells (`CellStyle.prose`: margin notes, labels, legends, under-line labels); code, line numbers, box borders and titles stay in JetBrains Mono. `--mono-labels` turns it off. In the terminal the user's font decides; JetBrains Mono (or any humanist mono) pairs well with the serif in the exported stills.
Emoji render as monochrome outlines when the fallback has them and are otherwise left blank: pass `--system-fonts` to also use the fonts installed on the machine.
