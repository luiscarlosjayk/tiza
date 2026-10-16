# Examples

Open them with `codedraw examples/<file>.codedraw` and advance with `n`. Both start at step 0 (code only).

| File | What it shows | `code` mode |
|---|---|---|
| `ownership.rs.codedraw` | move, borrow `&`, `&mut` and drop in 6 steps (two of them spotlights), with one color per value | relative reference: `{"file": "ownership.rs"}` |
| `stack-impl.codedraw` | `&self` vs `&mut self` in an `impl`, `Option<&T>` | reference with a subdirectory and a range: `{"file": "src/stack.rs", "lines": "7-30"}` |

## The three `code` modes

```jsonc
// 1. Reference relative to the .codedraw directory (what sidecars create)
"code": { "file": "src/stack.rs", "lines": "7-30" }

// 2. Absolute reference
"code": { "file": "/Users/luis/projects/app/src/main.rs" }

// 3. Embedded: portable, frozen (`:embed` inside the TUI)
"code": ["fn main() {", "    println!(\"hello\");", "}"]
```

`lines` is optional and accepts `"12-48"`, `"12..48"` or `"7"`. With a reference, the document always shows the current file and the numbering is the file's real one (it starts at `lines`), so you can say "look at line 20" and match the editor.

To generate a sidecar from any source: `codedraw src/main.rs:12-48` creates `src/main.rs.12-48.codedraw` in reference mode.
