# CodeDraw desde Zed

Objetivo: compartir pantalla con Zed, seleccionar código y mandarlo a CodeDraw en el terminal integrado (o en Ghostty) con un atajo.

## Tarea de Zed

En `~/.config/zed/tasks.json` (o `.zed/tasks.json` del proyecto):

```json
[
  {
    "label": "CodeDraw: archivo actual",
    "command": "codedraw",
    "args": ["$ZED_FILE"],
    "use_new_terminal": true,
    "allow_concurrent_runs": false,
    "reveal": "always"
  },
  {
    "label": "CodeDraw: selección",
    "command": "sh",
    "args": ["-c", "printf '%s' \"$ZED_SELECTED_TEXT\" | codedraw --lang \"${ZED_FILE##*.}\""],
    "use_new_terminal": true,
    "reveal": "always"
  },
  {
    "label": "CodeDraw: desde la línea actual",
    "command": "codedraw",
    "args": ["$ZED_FILE:$ZED_ROW"],
    "use_new_terminal": true,
    "reveal": "always"
  }
]
```

Atajo en `~/.config/zed/keymap.json`:

```json
[
  { "context": "Editor", "bindings": { "cmd-shift-d": ["task::Spawn", { "task_name": "CodeDraw: archivo actual" }] } }
]
```

Notas:

- Las tareas que usan `$ZED_SELECTED_TEXT` solo aparecen cuando hay texto seleccionado.
- `codedraw archivo.rs:A-B` conserva la numeración real de las líneas, así puedes decir “mira la línea 42” y coincidir con el editor.
- En el terminal de Zed, usa `line_height: "standard"` en los ajustes del terminal para que los caracteres de caja encajen sin huecos.
- Tema: `t` alterna oscuro/claro para que CodeDraw combine con el tema de Zed que estés usando en el video.
