# ╭─ CodeDraw ─▶

**Draw on top of code in the terminal to explain it on video.**

CodeDraw is a Rust TUI (ratatui) built for tutorials: you share your screen with your editor, send the code to CodeDraw in a terminal and draw on top of it (boxes, arrows, highlights, notes and semantic marks such as *warn*, *read*, *write*, *move* or *drop*, plus any style you define, emoji included) to explain a concept step by step. Annotations are anchored to the code text, not to screen cells, so they survive scrolling, resizing and theme changes.

It is also **for agents**: it exposes a local socket API and an **MCP server**, so Claude Code (or any MCP client) can draw live on what you are showing.

![CodeDraw explaining Rust ownership step by step](docs/img/ownership.gif)

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
codedraw src/main.rs --follow   # reload when the file is saved in the editor; annotations follow their text
pbpaste | codedraw              # code from the clipboard (macOS); --lang rs when it cannot be guessed
codedraw examples/ownership.rs.codedraw --present   # open the demo in presentation mode
```

Try the bundled demos (see [examples/README.md](examples/README.md)): `codedraw examples/ownership.rs.codedraw` starts with the bare code; advance with `n`.

### Keys

| Key | Action |
|---|---|
| `j`/`k`, `↑`/`↓`, `g`/`G` | move the line cursor |
| `h`/`l`, `w`/`b`/`e`, `0`/`$` | move the column cursor (word motions) |
| `v` / `V` / `i` | select text / select lines / select the word under the cursor |
| with a selection: `h` `m` `n` `C` `b` `a` `s` | highlight (text selection; a line selection makes a focus) · mark (then type `KIND [label]`) · note · callout (then type the text) · box · arrow (source `a`, target `a`) · spotlight |
| `Tab` / `Shift-Tab` | select an annotation; then `Enter` edit label · `e` set properties · `x` delete · `c` color · `+`/`-` move step |
| `n` / `p` (or `→`/`←`) | next / previous step; step 0 shows only the code |
| `u` / `U` | undo / redo |
| `t` | dark ⇄ light theme (like Excalidraw) |
| `P` | presentation mode (hides the UI) |
| `L` | annotation list · `?` help · `:` commands · `q` quit (`:q!` if the last save failed) |

### `:` commands

`L` is a line number and `text` an exact fragment to find on that line (`.` is the cursor line).

```
:mark warn 3:s careful here           built-in kinds: info warn error ok question important read write move new drop scope flow todo
:mark 🔥 4:res.json hot path          one-off mark: any glyph/emoji + your legend
:style io glyph=⇄ legend="network I/O" color=cyan   reusable style, then :mark io 2:fetch
:arrow 2:s 15:s: String move          arrow between two points, with a label
:arrow 7:&t 19:s borrow stroke=dashed  arrow styles: stroke=solid|dashed|dotted|thick|double  route=rounded|sharp  heads=START,END (none arrow dot bar open)
:set a2 stroke=thick end=bar          change properties of an annotation (label, color, step; arrows: stroke, route, heads, start, end; boxes/callouts: border, corners, fill)
:box 15-17 takes owns s               box around a range of lines
:box 15-17 takes border=double corners=sharp fill=no   borders: border=solid|dashed|dotted|thick|double  corners=rounded|sharp  fill=yes|no (callouts take the same keys)
:hl 7:&t shared borrow                highlight
:hl 7:&t shared emphasis=underline      how the span is drawn: fill (tint, default) · underline (▔ under it) · brackets (⌈ ⌉) · strike
:note 4 s no longer owns it           note anchored to a line
:callout 15-25 @auto title="why" two or three sentences…   bordered paragraph; @margin (default) | @below | @auto
:step 2        :step a3 2             change step · move annotation a3 to step 2
:del a1        :clear | :clear 2       delete one · clear everything or only one step (u undoes)
:label a2 new text                     change the label
:color a2 blue   :color a3 #ff8800   color per annotation (orange blue green purple pink cyan yellow red gray · auto)
:spot 15-17 a2 a3   :spot a6 a7      spotlight: in this step everything but those lines/annotations is dimmed
:until s1 4                            spotlight s1 stays active until step 4
:focus 15-17   :focus                 dim everything but a range · clear the focus
:theme light   :lang ts | :lang auto   :pad 1 4   :w   :q      (:pad = blank margins, saved in the document; :pad 0 0 none, :pad default)
:follow on | :follow off | :follow        reload the code when the source file is saved · state
:export shot.txt | :export shot.md | :export shot.png [step=N] [width=N] [theme=light]   export what a step shows (.txt .ansi .md .svg .png)
```

The language is detected from the file extension, then well-known names (`Makefile`, `Dockerfile`, `Cargo.lock`, `.bashrc`…), then a `#!` line, then a Vim/Emacs modeline in the first or last five lines; pasted or piped code uses the same shebang/modeline rule. `--lang` and `:lang <id>` override it (saved in the document); `:lang auto` re-detects and says how; an id without a grammar is shown as plain text with a warning.

New annotations appear at the current step. The `.codedraw` references the source file by path (`"code": {"file": "main.rs", "lines": "12-48"}`, absolute or relative to the `.codedraw`), so it always shows the current code; `:embed` freezes a copy inside the document and `:link path[:A-B]` references a file again. Color is yours, as in Excalidraw: every annotation accepts `color` (a name or `#rrggbb`); arrows without a color rotate through a palette so they can be told apart. Arrow strokes and heads are yours too: dashed for a shared borrow, thick for a move, `heads=none,none` for a plain connector — the ownership example does exactly that. Everything is saved automatically to a `.codedraw` (JSON) next to the source file (`src/main.rs.codedraw`).

**Follow mode** (`--follow` or `:follow on`) watches the source file's folder (Zed saves with a temp file + rename) and reloads the code about 120 ms after the last write, so you can edit in the editor while the TUI shows the result. Annotations are re-anchored by the text they point at, like a paste: a line diff matches the two versions in order (so a note on the second `}` stays on the second `}`), then the same occurrence of the span inside the line; a block moved elsewhere is followed when its line is unique and within 50 lines, a box or callout whose edge line you edited keeps its height, and the ones whose text is gone are dropped with a message — never moved to another copy — and `u` undoes the whole reload. The status line shows `⟳ following · last HH:MM:SSZ`; a removed file shows `waiting` until it comes back, an empty intermediate write is skipped, and embedded documents have nothing to follow (`:link path` first). Agents get the same through `follow.set` / `follow.status` and the MCP tool `set_follow`.

## Export to text, Markdown, ANSI, SVG or PNG

```sh
codedraw render demo.codedraw --out thumb.png --step 3          # PNG at 2× (thumbnails, posts)
codedraw render demo.codedraw --out figure.svg --theme light     # SVG with the fonts embedded
codedraw render --out shot.png                                   # what the live instance shows right now
codedraw render demo.codedraw --out notes.md --step 2            # fenced code block for a README, an issue or a chat
codedraw render demo.codedraw --out - --format ansi | less -R    # 24-bit ANSI, no trailing spaces
```

The text formats keep the box-drawing characters as they are on screen (`.txt`: plain, no trailing spaces or blank rows; `.md`: the same inside a ```` ```text ```` block with the document title above; `.ansi`: 24-bit colors with a reset per line, for `cat`, `less -R` or a terminal-recording tool). Inside the TUI, `:export shot.md` (or `.txt`, `.ansi`, `.svg`, `.png`, with `step=`, `width=`, `theme=`) writes the same next to the document.

Options: `--width` (columns, default 100 or the live terminal's), `--font-size` (16 px), `--margin` (1.5 cells), `--radius` (12 px, `0` for square corners), `--scale` (PNG, 2), `--no-embed-fonts` (small SVG that uses the viewer's fonts), `--system-fonts` (PNG fallback for emoji/CJK). The fonts ship inside the binary — JetBrains Mono (OFL) and a symbols subset of DejaVu Sans Mono — so the output is identical on every machine. Agents get the same through the `export_image` MCP tool (which also writes `.txt`, `.ansi` and `.md`).

![ownership example, step 7](docs/img/ownership-step7.png)

The GIF at the top of this README is eight of these exports (`--all-steps` is t-101; today it is a loop over `--step`) stitched with Pillow — no screen recording involved.

## For agents: MCP and local API

With the TUI open, start the MCP server in another process:

```sh
codedraw mcp
```

Configuration for Claude Code (the project's `.mcp.json` or `claude mcp add codedraw -- codedraw mcp`):

```json
{ "mcpServers": { "codedraw": { "command": "codedraw", "args": ["mcp"] } } }
```

Tools: `status`, `get_code`, `list_annotations`, `add_mark`, `define_style`, `list_kinds`, `add_arrow`, `add_box`, `add_note`, `add_callout`, `highlight`, `add_spotlight`, `update_annotation`, `delete_annotation`, `clear_annotations`, `set_step`, `next_step`, `prev_step`, `set_focus`, `set_theme`, `set_follow`, `render_text`, `export_image`. Anchors accept `{"find": "takes(s)"}` in addition to line and column, because an agent rarely knows the exact column; `update_annotation` moves an existing annotation with the same anchors, keeping its kind and style. Full guide: [docs/agent-guide.md](docs/agent-guide.md).

**After rebuilding CodeDraw** (`git pull && cargo build`, `cargo install`): an MCP client reads the tool list once per connection and keeps talking to the `codedraw mcp` process it started, which is still the old binary. New or changed tools appear only after the client reconnects — in Claude Code: `/mcp` → `codedraw` → *Reconnect* (or restart the session). The `status` tool says when this is needed: its `bridge` object carries the server's `version` and `build` (git commit, also in `codedraw --version`) and sets `binary_rebuilt: true` with a hint once the binary on disk is newer than the running server, or `instance_build` when the TUI was built from another commit (restart the TUI too). `notifications/tools/list_changed` would not help here: the stale process cannot know the new one's tools.

The contract is published as JSON Schema: [`schema/codedraw.schema.json`](schema/codedraw.schema.json) (the document) and [`schema/api.schema.json`](schema/api.schema.json) (every socket method, params and result), regenerated with `codedraw schema document|api` and checked in CI; the live instance also answers `api.schema`. `-` means stdin everywhere: `cat demo.codedraw | codedraw render - --out -`, `pbpaste | codedraw render - --lang rs --out shot.png`, `codedraw api annotation.add -`.

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

Full-screen terminal or 50 % next to Zed, 18–22 px font, snippets of ≤ 60 columns. The status line shows `cols×rows` and a ⚠ badge when the terminal is under 100×30 or a code line is clipped (`status` over the socket lists the same `warnings`); below ~100 columns, notes are drawn under the line instead of in the margin, and callouts with `placement: auto` move below their lines. One blank column is kept at each side by default (`:pad V H` to change it, `:pad 0 0` to remove it, `:pad default` to go back). Both themes use explicit RGB colors designed for 1080p.

## Development

```sh
cargo test                                   # core, TUI (incl. socket), commands and the per-ticket QA tests (qa/)
cargo clippy --all-targets -- -D warnings
cargo run -- examples/ownership.rs
```

Crates: `codedraw-core` (model, anchors, layout, rendering, protocol; no terminal dependencies), `codedraw-tui` (ratatui + crossterm + socket server), `codedraw-mcp` (MCP bridge with [rmcp](https://github.com/modelcontextprotocol/rust-sdk)), `codedraw-export` (SVG/PNG with [resvg](https://github.com/linebender/resvg) and embedded fonts), `codedraw-cli` (the `codedraw` binary). Decisions in [docs/adr](docs/adr). Dependency policy (maintained crates only, `cargo-deny` in CI): [docs/adr/0004-dependencias.md](docs/adr/0004-dependencias.md).

## Name

There is a Java library called [CodeDraw](https://github.com/Krassnig/CodeDraw) (drawing for beginners). They are unrelated; this project is a Rust terminal tool.

## License

MIT or Apache-2.0, at your option.
