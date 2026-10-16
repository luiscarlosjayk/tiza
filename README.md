# ╭─ CodeDraw ─▶

**Dibuja encima de código en la terminal para explicarlo en video.**

CodeDraw es una TUI en Rust (ratatui) pensada para tutoriales: compartes pantalla con tu editor, mandas el código a CodeDraw en una terminal y dibujas encima (cajas, flechas, resaltes, notas y marcas semánticas de Rust como *move*, *borrow* o *drop*) para explicar un concepto por pasos. Las anotaciones se anclan al texto del código, no a celdas de pantalla, así que sobreviven al scroll, al cambio de tamaño y al cambio de tema.

También es **para agentes**: expone una API local por socket y un **servidor MCP**, así que Claude Code (o cualquier cliente MCP) puede dibujar en vivo sobre lo que estás mostrando.

> Estado: **v0.0.1, primer demo funcional**. Inspirado en [Excalidraw](https://excalidraw.com), [excalidraw/cli](https://github.com/excalidraw/cli), [herdr](https://herdr.dev) y [tuios](https://github.com/Gaurav-Gosain/tuios).

```
 1          fn main() {
 2 ╭───────     let s = String::from("hola");
   │                ◀───╯                                                      nace s
 3 │            takes(s);                                                      ━▶ move: s pasa a takes
 4 │            // println!("{s}"); // error: value borrowed here after move   ┆ s ya no es dueña
 5 │
 6 │            let t = String::from("mundo");
 7 │   ╭───     let n = len(&t);                                               ╌▶ borrow &: préstamo
 8 │   │        println!("{t} tiene {n} letras");                              ┆ t sigue siendo válida
   ⋮   ⋮
   │ │ │ ╭─ takes es dueña de s y la libera al final ────────────────────────╮
15 ╰─│─│──▶ fn takes(s: String) {                                            │ move
16   │ │ │      println!("{s}");                                             │
17   │ │ │  }                                                                │ ╳ drop: aquí muere s
     │ │ ╰───────────────────────────────────────────────────────────────────╯
```

## Instalación

Binarios para macOS (Apple Silicon e Intel) y Linux en [Releases](https://github.com/luiscarlosjayk/codedraw/releases). O desde el código (Rust ≥ 1.88):

```sh
cargo install --git https://github.com/luiscarlosjayk/codedraw codedraw-cli
```

## Uso rápido

```sh
codedraw src/main.rs            # abre un archivo
codedraw src/main.rs:12-48      # solo un rango de líneas (la numeración se conserva)
pbpaste | codedraw --lang rs    # código desde el portapapeles (macOS)
codedraw examples/ownership.rs.codedraw --present   # abre el demo en modo presentación
```

Prueba el demo incluido: `codedraw examples/ownership.rs.codedraw` y avanza con `n`.

### Teclas

| Tecla | Acción |
|---|---|
| `j`/`k`, `↑`/`↓`, `g`/`G` | mover el cursor de línea |
| `n` / `p` (o `→`/`←`) | paso siguiente / anterior |
| `u` / `U` | deshacer / rehacer |
| `t` | tema oscuro ⇄ claro (como Excalidraw) |
| `P` | modo presentación (oculta la UI) |
| `l` | lista de anotaciones · `?` ayuda · `:` comandos · `q` salir |

### Comandos `:`

`L` es un número de línea y `texto` un fragmento exacto a buscar en esa línea (`.` es la línea del cursor).

```
:mark move 3:s s pasa a takes        marca semántica: move | borrow | mut | drop | life
:arrow 2:s 15:s: String move          flecha entre dos puntos, con etiqueta
:box 15-17 takes es dueña de s        caja alrededor de un rango de líneas
:hl 7:&t préstamo                     resalte
:note 4 s ya no es dueña              nota anclada a una línea
:step 2        :step a3 2             cambiar de paso · mover la anotación a3 al paso 2
:del a1        :label a2 nuevo texto  borrar · cambiar etiqueta
:focus 15-17   :focus                 atenuar todo salvo un rango · quitar el enfoque
:theme light   :lang ts   :w   :q
```

Las anotaciones nuevas aparecen en el paso actual. Todo se guarda automáticamente en un `.codedraw` (JSON) junto al archivo fuente (`src/main.rs.codedraw`).

## Para agentes: MCP y API local

Con la TUI abierta, arranca el servidor MCP en otro proceso:

```sh
codedraw mcp
```

Configuración para Claude Code (`.mcp.json` del proyecto o `claude mcp add codedraw -- codedraw mcp`):

```json
{ "mcpServers": { "codedraw": { "command": "codedraw", "args": ["mcp"] } } }
```

Herramientas: `status`, `get_code`, `list_annotations`, `add_mark`, `add_arrow`, `add_box`, `add_note`, `highlight`, `update_annotation`, `delete_annotation`, `set_step`, `next_step`, `prev_step`, `set_focus`, `set_theme`, `render_text`. Las anclas aceptan `{"find": "takes(s)"}` además de línea y columna, porque un agente rara vez conoce la columna exacta. Guía completa: [docs/agent-guide.md](docs/agent-guide.md).

La misma API está disponible como socket Unix con JSON por líneas (diseño tomado de herdr) y desde el CLI:

```sh
codedraw scene status                 # ¿hay una instancia? ¿qué muestra?
codedraw scene list                   # anotaciones
codedraw scene text                   # lo que se ve, como texto
codedraw api annotation.add '{"type":"note","line":4,"text":"hola"}'
codedraw render demo.codedraw --out - --step 2 --width 100   # sin abrir la TUI
```

Varias instancias: `--session nombre` (o `CODEDRAW_SESSION`). Socket explícito: `--socket ruta` (o `CODEDRAW_SOCKET`). El socket vive en `$XDG_RUNTIME_DIR` o `$TMPDIR` con permisos 0600 y **nunca** escucha en TCP. CodeDraw no envía nada a ningún sitio.

## Integración con Zed

Una tarea que manda la selección actual a CodeDraw en el terminal integrado: [docs/zed.md](docs/zed.md).

## Perfil de grabación recomendado

Terminal a pantalla completa o al 50 % junto a Zed, fuente de 18–22 px, snippets de ≤ 60 columnas. La línea de estado muestra `cols×rows`; con menos de ~100 columnas las notas se dibujan debajo de la línea en vez de en el margen. Los dos temas usan colores RGB explícitos pensados para 1080p.

## Desarrollo

```sh
cargo test                                   # núcleo, TUI (incl. socket) y comandos
cargo clippy --all-targets -- -D warnings
cargo run -- examples/ownership.rs
```

Crates: `codedraw-core` (modelo, anclas, layout, render, protocolo; sin dependencias de terminal), `codedraw-tui` (ratatui + crossterm + servidor del socket), `codedraw-mcp` (puente MCP con [rmcp](https://github.com/modelcontextprotocol/rust-sdk)), `codedraw-cli` (binario `codedraw`). Decisiones en [docs/adr](docs/adr). Política de dependencias (solo crates mantenidos, `cargo-deny` en CI): [docs/adr/0004-dependencias.md](docs/adr/0004-dependencias.md).

## Nombre

Existe una librería Java llamada [CodeDraw](https://github.com/Krassnig/CodeDraw) (dibujo para principiantes). No tienen relación; este proyecto es una herramienta de terminal en Rust.

## Licencia

MIT o Apache-2.0, a tu elección.
