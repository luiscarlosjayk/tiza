# Architecture Decision Records

One decision per file, kept short. Format: context, decision, consequences, status.

| ADR | Decision |
|---|---|
| [0001](0001-tui-ratatui.md) | CodeDraw is a TUI built with ratatui, not a graphical app |
| [0002](0002-anclas-y-lenguaje-visual.md) | Anchors by (line, grapheme) and a visual language of boxes, arrows and notes |
| [0003](0003-api-socket-y-mcp.md) | Local NDJSON API over a Unix socket; the MCP is a thin bridge |
| [0004](0004-dependencias.md) | Dependency policy: maintained crates only, audited in CI |
| [0005](0005-formato-codedraw.md) | `.codedraw` format: versioned JSON, sidecar next to the source |
| [0006](0006-exportacion-svg-png.md) | SVG and PNG export: Canvas → SVG text grid, rasterized with resvg, fonts embedded |
| [0007](0007-moves-y-pasos-con-nombre.md) | Moves (code.patch, code.link, cursor, select, wait) and named steps: scripted demos without an editor (proposed) |
