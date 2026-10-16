# Para agentes que trabajan en este repo

- Workspace Cargo con 4 crates: `codedraw-core` (modelo/layout, sin terminal), `codedraw-tui`, `codedraw-mcp`, `codedraw-cli` (binario `codedraw`).
- Antes de proponer un cambio: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`.
- Decisiones de diseño en `docs/adr/`. Si cambias el protocolo del socket, actualiza `docs/agent-guide.md` y las herramientas MCP a la vez.
- Nuevas dependencias: sigue `docs/adr/0004-dependencias.md` y añade la fila al registro.
- Commits en español, imperativo, con el id de tarea del board cuando aplique (`t-022`).
- Para probar la TUI sin terminal interactiva usa un pseudo-terminal (Python `pty`) y la API: `codedraw api status --socket /tmp/x.sock`.
