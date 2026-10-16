# Embedded fonts

Used only by `codedraw render --format svg|png` so the exported image looks the same on every machine.

| File | Family | License | Notes |
|---|---|---|---|
| `JetBrainsMono-{Regular,Bold,Italic,BoldItalic}.ttf` | JetBrains Mono 2.304 | [OFL 1.1](JetBrainsMono-OFL.txt) | code, labels, box drawing, arrows |
| `DejaVuSansMono-Symbols.ttf` | DejaVu Sans Mono 2.37, subset to U+2000–2BFF and U+1F300–1FAFF | [DejaVu license](DejaVu-LICENSE.txt) (Bitstream Vera + public domain) | fallback for symbols JetBrains Mono lacks (★ ✎ ⇄ …) |

The subset was produced with `pyftsubset DejaVuSansMono.ttf --unicodes="U+2000-2BFF,U+1F300-1FAFF" --no-hinting`.
Emoji render as monochrome outlines when the fallback has them and are otherwise left blank: pass `--system-fonts` to also use the fonts installed on the machine.
