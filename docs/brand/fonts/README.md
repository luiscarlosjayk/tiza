# Brand fonts

Brand material only: these files are **not compiled into `tiza` and not shipped in any crate** (they live outside every crate directory, so `cargo package` never sees them). They are here for the brand work — the README wordmark, thumbnails and video titles — and for anyone who needs the brand serif outside the app. Every export of the binary (SVG, PNG, GIF, video) is set in JetBrains Mono, as in the terminal; the serif was taken out of the binary on 2026-10-18 ([ADR 0006](../../adr/0006-exportacion-svg-png.md)).

| Files | Family | License |
|---|---|---|
| `TizaSerif-{Regular,Bold,Italic,BoldItalic}.ttf` | [Libron](https://github.com/nicoverbruggen/libron) 0.31, subset to Latin (U+0020–007E, U+00A0–024F, U+2000–206F, U+20AC, U+2122, U+2190–21FF, U+2026), renamed *Tiza Serif* | [OFL 1.1](LICENSE-TizaSerif-Libron-OFL.txt) |

Provenance and the subset command: `Libron.zip` of release v0.31 (the repo holds only FontForge sources), `pyftsubset Libron-Regular.ttf --unicodes="U+0020-007E,U+00A0-024F,U+2000-206F,U+20AC,U+2122,U+2190-21FF,U+2026" --no-hinting` for each of the four faces, then the family renamed with fontTools. It was first distributed as *CodeDraw Serif* and renamed with the project (ADR 0008).

**Tiza Serif is Libron, renamed.** The OFL declares "Libron" a Reserved Font Name, so a subset (a modified version) must not be distributed under that name: the `name` table says *Tiza Serif* (`TizaSerif-*`), the copyright and license text are kept unchanged (the license file explains the rename), and the original is at github.com/nicoverbruggen/libron. For a design that needs glyphs outside the subset, use the original Libron release under its own name instead of extending these files.
