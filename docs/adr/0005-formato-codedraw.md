# 0005 · Formato `.codedraw`

**Estado:** aceptada (2026-10-15)

## Decisión

- JSON legible, con `schema` (entero) desde el día uno. Esta versión escribe `schema: 1` y rechaza esquemas mayores con un error claro.
- El código viaja **embebido** (`code: [líneas]`) junto con `source.path`, `source.language` y `source.first_line`: el documento se puede renderizar sin el archivo original y conserva la numeración real.
- Anotaciones con `id`, `step` y la forma aplanada con `type` (`highlight`, `mark`, `box`, `arrow`, `note`).
- **Sidecar**: al abrir `src/main.rs` se guarda en `src/main.rs.codedraw`; con rango, `src/main.rs.12-48.codedraw`. Si el sidecar existe, se abre él (conserva anotaciones).
- Guardado **automático y atómico** (tmp + rename) tras cada cambio; `:w [ruta]` para elegir otro destino.

## Consecuencias

- Migraciones: cuando cambie el esquema, `fs::parse_document` aplicará transformaciones por versión; fixtures por versión en los tests (t-073).
- El `find` original de un ancla se guardará en una versión futura para re-anclar tras ediciones del fuente (t-011/t-043).
