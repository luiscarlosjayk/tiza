# 0003 · API local NDJSON por socket Unix; el MCP es un puente delgado

**Estado:** aceptada (2026-10-15)

## Contexto

Requisito: agentes (Claude Code, Zed) deben poder crear, editar y borrar anotaciones en la instancia viva mientras el presentador comparte pantalla. herdr resuelve el mismo problema con un servidor headless, un socket Unix con JSON por líneas y un CLI delgado encima; excalidraw/cli muestra el estilo de CLI "para humanos y agentes" (JSON por defecto, `-` para stdin/stdout).

## Decisión

- La **TUI es la única dueña del documento** y expone una API local: socket Unix, **una petición JSON por línea** `{id, method, params}` → `{id, result}` o `{id, error: {code, message}}`; métodos con puntos (`annotation.add`, `step.set`, `render.text`); `ping` y `status` para descubrimiento; los clientes ignoran campos desconocidos. Códigos de error estables (`out_of_range`, `not_found`, `duplicate_id`, `text_not_found`, `invalid_argument`…).
- **Resolución del socket**: `--socket` > `$CODEDRAW_SOCKET` > sesión (`--session`/`$CODEDRAW_SESSION`, por defecto `default`) en `$XDG_RUNTIME_DIR` o `$TMPDIR` como `codedraw/<sesión>.sock`. Directorio 0700, socket 0600, **nunca TCP**. Sockets huérfanos se limpian al arrancar; una instancia viva en la misma ruta es un error claro (usa `--session`).
- **MCP** (`codedraw mcp`, stdio, crate `rmcp` 3.x): cada herramienta es exactamente un método del socket. Si no hay instancia, la herramienta devuelve un error que explica cómo abrirla. Tipos de parámetros con `schemars` para que el agente reciba un JSON Schema real.
- El **CLI** usa el mismo socket (`codedraw scene …`, `codedraw api <método> <json>`) y `codedraw render` dibuja sin instancia, con el mismo motor de `core`.
- Implementación con `std` (hilo por conexión + canal al hilo de la TUI); sin tokio en core ni en la TUI. tokio solo en el puente MCP, porque `rmcp` lo requiere.

## Consecuencias

- Una sola instancia por sesión; varias sesiones con `--session`.
- No hay autenticación más allá de los permisos del socket: suficiente para una herramienta local de un solo usuario; documentado en el README (cero telemetría).
- `codedraw api schema --json` (JSON Schema del protocolo) queda como seguimiento (t-040).
