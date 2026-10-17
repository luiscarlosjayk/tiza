# Architecture Decision Records

One decision per file, kept short. Format: context, decision, consequences, status.

| ADR | Decision |
|---|---|
| [0001](0001-tui-ratatui.md) | A TUI built with ratatui, not a graphical app |
| [0002](0002-anclas-y-lenguaje-visual.md) | Anchors by (line, grapheme) and a visual language of boxes, arrows and notes |
| [0003](0003-api-socket-y-mcp.md) | Local NDJSON API over a Unix socket; the MCP is a thin bridge |
| [0004](0004-dependencias.md) | Dependency policy: maintained crates only, audited in CI |
| [0005](0005-formato-codedraw.md) | `.tiza` format: versioned JSON, sidecar next to the source |
| [0006](0006-exportacion-svg-png.md) | SVG and PNG export: Canvas → SVG text grid, rasterized with resvg, fonts embedded |
| [0007](0007-moves-y-pasos-con-nombre.md) | Moves (code.patch, code.link, cursor, select, wait) and named steps: scripted demos without an editor (accepted; deterministic re-anchoring, `step: N`, follow paused while moves are applied) |
| [0008](0008-renombre-a-tiza.md) | The project is renamed from CodeDraw to Tiza: `tiza` crate, binary, socket, `.tiza` sidecar, MCP server; v0.0.1 is re-tagged after the rename |

ADRs 0001–0007 predate the rename and still say CodeDraw; each carries a note at the top.
