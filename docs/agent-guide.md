# Guía para agentes

Esta página es para un agente (Claude Code, Zed, cualquier cliente MCP) que vaya a dibujar sobre el código que un presentador tiene abierto en CodeDraw.

## Modelo mental

- La **TUI es la única dueña del documento**. Tú hablas con ella; no editas el `.codedraw` a mano mientras está abierta.
- Un documento tiene **código** (líneas numeradas, numeración absoluta aunque se abra un rango) y **anotaciones**. Cada anotación tiene un **id** corto y estable (`a1`, `a2`… o el que tú pases) y un **paso** (1-based). Se muestran las anotaciones con `step <= paso actual`.
- Las **anclas** son posiciones en el texto: `{line, col}` o, mejor, `{find: "texto exacto", line?: N, occurrence?: K}`. `find` busca el texto en la línea indicada (si la das) y después en todo el documento. Si no existe, la herramienta falla con `text_not_found`: no adivines columnas.

## Flujo recomendado

1. `status` — confirma que hay una instancia y mira `cols`/`rows` (si `cols < 100`, las etiquetas van debajo de la línea: hazlas cortas).
2. `get_code` — lee el código con números de línea. Elige textos exactos.
3. Anota en orden de explicación, asignando `step` creciente:
   - `add_mark` para semántica de Rust (`move`, `borrow`, `borrow_mut`, `drop`, `lifetime`): cada tipo tiene color y glifo fijos y muestra su leyenda.
   - `add_arrow` para relaciones (de donde nace un valor a donde se consume).
   - `add_box` para delimitar una función o un bloque.
   - `add_note` para una frase corta; `highlight` para un fragmento sin semántica.
4. `set_step 0` deja solo el código (buen punto de partida para la toma); luego `next_step` conforme el presentador avanza (o déjalo en sus manos: `n`/`p`).
5. `render_text` para ver exactamente lo que ve la audiencia y corregir.

## Ejemplo (ownership)

```json
{"name":"add_mark","arguments":{"kind":"move","at":{"line":3,"find":"s"},"label":"s pasa a takes","step":1}}
{"name":"add_arrow","arguments":{"from":{"line":2,"find":"s"},"to":{"line":15,"find":"s: String"},"label":"move","step":1,"color":"orange"}}
{"name":"add_note","arguments":{"line":4,"text":"s ya no es dueña: usarla aquí no compila","step":2}}
{"name":"add_box","arguments":{"from_line":15,"to_line":17,"label":"takes es dueña de s y la libera al final","step":2}}
```

## Color

Cada anotación acepta `color`: `orange`, `blue`, `green`, `purple`, `pink`, `cyan`, `yellow`, `red`, `gray` (también en español: `azul`, `verde`…) o `#rrggbb`. Úsalo para contar una historia por valor: todo lo que le pasa a `s` en un color, todo lo de `t` en otro. Las flechas sin color rotan una paleta automática; cajas, notas y resaltes sin color usan el del tema. `update_annotation` con `color: ""` vuelve al automático.

## Entre tomas

`clear_annotations` vacía el lienzo de golpe (o solo un paso con `step`). El presentador puede deshacerlo con `u`, así que no es destructivo en vivo.

## Idempotencia

Pasa tu propio `id` (`"id": "move-s"`) para que un reintento no duplique la anotación: el segundo intento falla con `duplicate_id` y puedes seguir.

## Sin MCP: socket y CLI

Mismo contrato, una petición JSON por línea sobre el socket Unix:

```sh
printf '%s\n' '{"id":1,"method":"annotation.add","params":{"type":"note","line":4,"text":"hola"}}' | nc -U "$TMPDIR/codedraw/default.sock"
codedraw api annotation.add '{"type":"note","line":4,"text":"hola"}'
codedraw api api.methods
```

Respuesta: `{"id":1,"result":{...}}` o `{"id":1,"error":{"code":"text_not_found","message":"..."}}`. Códigos: `out_of_range`, `not_found`, `duplicate_id`, `text_not_found`, `invalid_argument`, `invalid_json`, `unavailable`.
