# Ejemplos

Ábrelos con `codedraw examples/<archivo>.codedraw` y avanza con `n`. Los dos arrancan en el paso 0 (solo el código).

| Archivo | Qué muestra | Modo de `code` |
|---|---|---|
| `ownership.rs.codedraw` | move, borrow `&`, `&mut` y drop en 5 pasos, con un color por valor | referencia relativa: `{"file": "ownership.rs"}` |
| `stack-impl.codedraw` | `&self` vs `&mut self` en un `impl`, `Option<&T>` | referencia con subdirectorio y rango: `{"file": "src/stack.rs", "lines": "7-30"}` |

## Los tres modos de `code`

```jsonc
// 1. Referencia relativa al directorio del .codedraw (lo que crean los sidecars)
"code": { "file": "src/stack.rs", "lines": "7-30" }

// 2. Referencia absoluta
"code": { "file": "/Users/luis/proyectos/app/src/main.rs" }

// 3. Embebido: portable, congelado (`:embed` dentro de la TUI)
"code": ["fn main() {", "    println!(\"hola\");", "}"]
```

`lines` es opcional y acepta `"12-48"`, `"12..48"` o `"7"`. Con referencia, el documento muestra siempre el archivo actual y la numeración es la real del archivo (empieza en `lines`), así que puedes decir "mira la línea 20" y coincidir con el editor.

Para generar un sidecar desde cualquier fuente: `codedraw src/main.rs:12-48` crea `src/main.rs.12-48.codedraw` en modo referencia.
