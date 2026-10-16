# Changelog

Formato basado en [Keep a Changelog](https://keepachangelog.com/es/1.1.0/); versiones [SemVer](https://semver.org/lang/es/).

## [0.0.1] - 2026-10-16

Primer demo funcional.

### Añadido
- TUI (ratatui + crossterm): código con resaltado de sintaxis (syntect + two-face), cursor de línea, scroll, tema oscuro/claro conmutable, modo presentación, pasos, deshacer/rehacer, lista de anotaciones y ayuda.
- Anotaciones ancladas al texto: resaltes, marcas de Rust (move, borrow, &mut, drop, lifetime), cajas con filas virtuales, flechas por carril izquierdo y en la misma línea, notas en margen o debajo.
- Línea de comandos `:` (`:mark`, `:arrow`, `:box`, `:hl`, `:note`, `:step`, `:del`, `:label`, `:focus`, `:theme`, `:lang`, `:w`, `:q`…).
- Entrada: archivo, archivo con rango `a.rs:12-48`, stdin, pegado (bracketed paste).
- Formato `.codedraw` (JSON, `schema: 1`) con guardado automático atómico y sidecar junto al fuente.
- API local por socket Unix (NDJSON, estilo herdr) y CLI `codedraw scene|api|render`.
- Servidor MCP (`codedraw mcp`) con 16 herramientas, anclas por texto (`find`).
- Ejemplo `examples/ownership.rs.codedraw` (5 pasos), guía para agentes, integración con Zed, ADRs 0001–0005.
- CI (fmt, clippy, test en macOS y Linux, cargo-deny) y releases con binarios desde tags.
