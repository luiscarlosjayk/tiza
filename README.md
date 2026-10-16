# ╭─ CodeDraw ─▶

**Draw on top of code in the terminal to explain it on video.**

CodeDraw is a Rust TUI (ratatui) built for tutorials: you share your screen with your editor, send the code to CodeDraw in a terminal and draw on top of it (boxes, arrows, highlights, notes and semantic marks such as *warn*, *read*, *write*, *move* or *drop*, plus any style you define, emoji included) to explain a concept step by step. Annotations are anchored to the code text, not to screen cells, so they survive scrolling, resizing and theme changes.

It is also **for agents**: it exposes a local socket API and an **MCP server**, so Claude Code (or any MCP client) can draw live on what you are showing.

> Status: **v0.0.1, first working demo**. Inspired by [Excalidraw](https://excalidraw.com), [excalidraw/cli](https://github.com/excalidraw/cli), [herdr](https://herdr.dev) and [tuios](https://github.com/Gaurav-Gosain/tuios).

```
 1          fn main() {
 2 ╭───────     let s = String::from("hello");
   │                ◀───╯                                                      s is born
 3 │            takes(s);                                                      ━▶ move: s moves into
 4 │            // println!("{s}"); // error: value borrowed here after move   takes
 5 │                                                                           ┆ s no longer owns
 6 │            let t = String::from("world");                                   it: using it here
 7 │   ╭───     let n = len(&t);                                                 does not compile
 8 │   │        println!("{t} has {n} letters");                               ╌▶ borrow &: shared
 9 │   │                                                                       borrow
10 │   │        let mut u = String::from("hey");                               ┆ t is still valid
   ⋮   ⋮
   │ │ │ ╭─ takes owns s and frees it at the end ────────────────────────────╮
15 ╰─┼─┼─┼▶ fn takes(s: String) {                                            │ move
16   │ │ │      println!("{s}");                                             │
17   │ │ │  }                                                                │ ╳ drop: s dies here
     │ │ ╰───────────────────────────────────────────────────────────────────╯
```

## Installation

Binaries for macOS (Apple Silicon and Intel) and Linux in [Releases](https://github.com/luiscarlosjayk/codedraw/releases). Or from source (Rust ≥ 1.88):

```sh
cargo install --git https://github.com/luiscarlosjayk/codedraw codedraw-cli
```

## Quick start

```sh
codedraw src/main.rs            # open a file
codedraw src/main.rs:12-48      # only a range of lines (numbering is preserved)
pbpaste | codedraw --lang rs    # code from the clipboard (macOS)
codedraw examples/ownership.rs.codedraw --present   # open the demo in presentation mode
```

Try the bundled demos (see [examples/README.md](examples/README.md)): `codedraw examples/ownership.rs.codedraw` starts with the bare code; advance with `n`.

### Keys

| Key | Action |
|---|---|
| `j`/`k`, `↑`/`↓`, `g`/`G` | move the line cursor |
| `n` / `p` (or `→`/`←`) | next / previous step; step 0 shows only the code |
| `u` / `U` | undo / redo |
| `t` | dark ⇄ light theme (like Excalidraw) |
| `P` | presentation mode (hides the UI) |
| `l` | annotation list · `?` help · `:` commands · `q` quit |

### `:` commands

`L` is a line number and `text` an exact fragment to find on that line (`.` is the cursor line).

```
:mark warn 3:s careful here           built-in kinds: info warn error ok question important read write move new drop scope flow todo
:mark 🔥 4:res.json hot path          one-off mark: any glyph/emoji + your legend
:style io glyph=⇄ legend="network I/O" color=cyan   reusable style, then :mark io 2:fetch
:arrow 2:s 15:s: String move          arrow between two points, with a label
:box 15-17 takes owns s               box around a range of lines
:hl 7:&t shared borrow                highlight
:note 4 s no longer owns it           note anchored to a line
:step 2        :step a3 2             change step · move annotation a3 to step 2
:del a1        :clear | :clear 2       delete one · clear everything or only one step (u undoes)
:label a2 new text                     change the label
:color a2 blue   :color a3 #ff8800   color per annotation (orange blue green purple pink cyan yellow red gray · auto)
:spot 15-17 a2 a3   :spot a6 a7      spotlight: in this step everything but those lines/annotations is dimmed
:until s1 4                            spotlight s1 stays active until step 4
:focus 15-17   :focus                 dim everything but a range · clear the focus
:theme light   :lang ts   :w   :q
```

New annotations appear at the current step. The `.codedraw` references the source file by path (`"code": {"file": "main.rs", "lines": "12-48"}`, absolute or relative to the `.codedraw`), so it always shows the current code; `:embed` freezes a copy inside the document and `:link path[:A-B]` references a file again. Color is yours, as in Excalidraw: every annotation accepts `color` (a name or `#rrggbb`); arrows without a color rotate through a palette so they can be told apart. Everything is saved automatically to a `.codedraw` (JSON) next to the source file (`src/main.rs.codedraw`).

## For agents: MCP and local API

With the TUI open, start the MCP server in another process:

```sh
codedraw mcp
```

Configuration for Claude Code (the project's `.mcp.json` or `claude mcp add codedraw -- codedraw mcp`):

```json
{ "mcpServers": { "codedraw": { "command": "codedraw", "args": ["mcp"] } } }
```

Tools: `status`, `get_code`, `list_annotations`, `add_mark`, `define_style`, `list_kinds`, `add_arrow`, `add_box`, `add_note`, `highlight`, `add_spotlight`, `update_annotation`, `delete_annotation`, `clear_annotations`, `set_step`, `next_step`, `prev_step`, `set_focus`, `set_theme`, `render_text`. Anchors accept `{"find": "takes(s)"}` in addition to line and column, because an agent rarely knows the exact column. Full guide: [docs/agent-guide.md](docs/agent-guide.md).

The same API is available as a Unix socket with line-delimited JSON (design borrowed from herdr) and from the CLI:

```sh
codedraw scene status                 # is there an instance? what is it showing?
codedraw scene list                   # annotations
codedraw scene text                   # what is on screen, as text
codedraw api annotation.add '{"type":"note","line":4,"text":"hello"}'
codedraw render demo.codedraw --out - --step 2 --width 100   # without opening the TUI
```

Several instances: `--session name` (or `CODEDRAW_SESSION`). Explicit socket: `--socket path` (or `CODEDRAW_SOCKET`). The socket lives in `$XDG_RUNTIME_DIR` or `$TMPDIR` with 0600 permissions and **never** listens on TCP. CodeDraw sends nothing anywhere.

## Zed integration

A task that sends the current selection to CodeDraw in the integrated terminal: [docs/zed.md](docs/zed.md).

## Recommended recording profile

Full-screen terminal or 50 % next to Zed, 18–22 px font, snippets of ≤ 60 columns. The status line shows `cols×rows`; below ~100 columns, notes are drawn under the line instead of in the margin. Both themes use explicit RGB colors designed for 1080p.

## Development

```sh
cargo test                                   # core, TUI (incl. socket) and commands
cargo clippy --all-targets -- -D warnings
cargo run -- examples/ownership.rs
```

Crates: `codedraw-core` (model, anchors, layout, rendering, protocol; no terminal dependencies), `codedraw-tui` (ratatui + crossterm + socket server), `codedraw-mcp` (MCP bridge with [rmcp](https://github.com/modelcontextprotocol/rust-sdk)), `codedraw-cli` (the `codedraw` binary). Decisions in [docs/adr](docs/adr). Dependency policy (maintained crates only, `cargo-deny` in CI): [docs/adr/0004-dependencias.md](docs/adr/0004-dependencias.md).

## Name

There is a Java library called [CodeDraw](https://github.com/Krassnig/CodeDraw) (drawing for beginners). They are unrelated; this project is a Rust terminal tool.

## License

MIT or Apache-2.0, at your option.
