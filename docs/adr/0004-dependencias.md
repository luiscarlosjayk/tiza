# 0004 · Política de dependencias

**Estado:** aceptada (2026-10-15)

## Decisión

Somos selectivos con los crates: solo entran los que siguen teniendo mantenimiento. Criterios por crate:

- Último release en los últimos 12 meses **o** repo con actividad reciente y issues atendidos.
- Más de un mantenedor u organización detrás (bus factor), o un crate pequeño que podríamos absorber.
- Licencia compatible (MIT, Apache-2.0, BSD, ISC, Zlib, CC0, MPL-2.0; lista en `deny.toml`).
- Sin dependencias C innecesarias (p. ej. `syntect` con `default-fancy` en vez de oniguruma).
- Preferimos `std` cuando basta (socket Unix, hilos, canales) antes que un runtime async.

Herramientas: `cargo-deny` en CI (licencias, advisories, fuentes, duplicados), `Cargo.lock` versionado, `--locked` en releases. Cada crate nuevo se anota aquí con la fecha de evaluación.

## Registro (2026-10-15)

| Crate | Versión | Último release | Licencia | Notas |
|---|---|---|---|---|
| ratatui / ratatui-core | 0.30.2 / 0.1.2 | 2026-06 | MIT | MSRV 1.88; crossterm 0.29 por defecto |
| crossterm | 0.29.0 | 2025-04 | MIT | Repo activo; features `bracketed-paste`, `use-dev-tty` |
| syntect | 5.3.0 | 2025-09 | MIT | `default-fancy` (Rust puro) |
| two-face | 0.5.2 | 2025-12 | MIT/Apache-2.0 | Gramáticas de bat (TypeScript, TOML…) |
| rmcp | 3.5.1 | 2026-10 | Apache-2.0 | SDK oficial de MCP; releases frecuentes; API 3.x |
| schemars | 1.2 | 2026-10 | MIT | Requerido por rmcp 3.x |
| tokio | 1.53 | 2026-10 | MIT | Solo en `codedraw-mcp` |
| clap | 4.6 | 2026-09 | MIT/Apache-2.0 | derive |
| serde / serde_json | 1.0.229 / 1.0.154 | 2026-07 / 2026-10 | MIT/Apache-2.0 | |
| thiserror | 2 | — | MIT/Apache-2.0 | |
| unicode-width / unicode-segmentation | 0.2.2 / 1.13.3 | 2025-10 / 2026-06 | MIT/Apache-2.0 | Anclas por grafema, ancho por celda |
| anyhow | 1 | — | MIT/Apache-2.0 | Solo en binarios |

Evaluados y **pospuestos**: `notify` 8 (seguir archivo; Zed guarda con tmp+rename, vigilar el directorio), `arboard` (portapapeles dentro de la app; `pbpaste | codedraw` cubre macOS), `resvg` (PNG; requiere embeber fuentes), `insta`/`proptest` (entrarán con los snapshot tests).

### Excepciones registradas

- `bincode 1.3.3` (transitiva, `syntect` → `dump-load`): RUSTSEC-2025-0141 "unmaintained". Su equipo declaró la 1.3.3 completa. Solo deserializa las gramáticas precompiladas que viajan embebidas en el binario; no toca datos externos. Ignorada en `deny.toml`; revisar cuando `syntect` cambie de formato de dump.
- Se evitó `yaml-rust` (RUSTSEC-2024-0320) desactivando `yaml-load` en `syntect`: no cargamos gramáticas YAML en caliente.
