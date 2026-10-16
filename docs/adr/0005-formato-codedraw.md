# 0005 · `.codedraw` format

**Status:** accepted (2026-10-15)

## Decision

- Readable JSON, with `schema` (integer) from day one. This version writes `schema: 1` and rejects higher schemas with a clear error.
- `code` supports two modes:
  - **reference** (default for sidecars): `{"file": "src/main.rs", "lines": "12-48"}`. `file` may be absolute or relative to the `.codedraw` directory; `lines` is optional. The document always shows the current file; `source.first_line` and the language are inferred on load.
  - **embedded**: `code: ["line", …]`. Portable and frozen (useful for examples and for sharing). `:embed` freezes; `:link path[:A-B]` references a file again.
- `source.path`, `source.language` and `source.first_line` accompany both modes to preserve the real numbering.
- Annotations with `id`, `step` and the flattened shape with `type` (`highlight`, `mark`, `box`, `arrow`, `note`, `callout`, `spotlight`). A `callout` carries `from_line`, optional `to_line`, `text` (newlines are paragraph breaks), optional `title`, `placement` (`margin` default, `below`, `auto`; omitted when default) and optional `width` (max text columns).
- **Sidecar**: opening `src/main.rs` saves to `src/main.rs.codedraw`; with a range, `src/main.rs.12-48.codedraw`. If the sidecar exists, it is the one opened (it keeps the annotations).
- **Automatic, atomic** saving (tmp + rename) after every change; `:w [path]` to pick another destination.

## Consequences

- Migrations: when the schema changes, `fs::parse_document` will apply per-version transformations; per-version fixtures in the tests (t-073).
- The original `find` of an anchor will be stored in a future version to re-anchor after source edits (t-011/t-043).
