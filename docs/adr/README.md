# Architecture Decision Records

Una decisión por archivo, corta. Formato: contexto, decisión, consecuencias, estado.

| ADR | Decisión |
|---|---|
| [0001](0001-tui-ratatui.md) | CodeDraw es una TUI con ratatui, no una app gráfica |
| [0002](0002-anclas-y-lenguaje-visual.md) | Anclas por (línea, grafema) y lenguaje visual de cajas, flechas y notas |
| [0003](0003-api-socket-y-mcp.md) | API local NDJSON por socket Unix; el MCP es un puente delgado |
| [0004](0004-dependencias.md) | Política de dependencias: solo crates mantenidos, auditados en CI |
| [0005](0005-formato-codedraw.md) | Formato `.codedraw`: JSON versionado, sidecar junto al fuente |
