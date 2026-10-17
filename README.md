# ╭─ Tiza ─▶

**Draw on top of code in the terminal to explain it on video.**

Tiza is a Rust TUI (ratatui) built for tutorials: you share your screen with your editor, send the code to Tiza in a terminal and draw on top of it (boxes, arrows, highlights, notes and semantic marks such as *warn*, *read*, *write*, *move* or *drop*, plus any style you define, emoji included) to explain a concept step by step. Annotations are anchored to the code text, not to screen cells, so they survive scrolling, resizing and theme changes.

It is also **for agents**: it exposes a local socket API and an **MCP server**, so Claude Code (or any MCP client) can draw live on what you are showing.

![Tiza explaining Rust ownership step by step](docs/img/ownership.gif)

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

Binaries for macOS (Apple Silicon and Intel) and Linux in [Releases](https://github.com/luiscarlosjayk/tiza/releases). Or from source (Rust ≥ 1.88):

```sh
cargo install --git https://github.com/luiscarlosjayk/tiza tiza
```

## Quick start

```sh
tiza src/main.rs            # open a file
tiza src/main.rs:12-48      # only a range of lines (numbering is preserved)
tiza src/main.rs --follow   # reload when the file is saved in the editor; annotations follow their text
pbpaste | tiza              # code from the clipboard (macOS); --lang rs when it cannot be guessed
tiza examples/ownership.rs.tiza --present   # open the demo in presentation mode
```

Try the bundled demos (see [examples/README.md](examples/README.md)): `tiza examples/ownership.rs.tiza` starts with the bare code; advance with `n`.

### Keys

| Key | Action |
|---|---|
| `j`/`k`, `↑`/`↓`, `g`/`G` | move the line cursor |
| `h`/`l`, `w`/`b`/`e`, `0`/`$` | move the column cursor (word motions) |
| `v` / `V` / `i` | select text / select lines / select the word under the cursor |
| with a selection: `h` `m` `n` `C` `b` `a` `s` | highlight (text selection; a line selection makes a focus) · mark (then type `KIND [label]`) · note · callout (then type the text) · box · arrow (source `a`, target `a`) · spotlight |
| `Tab` / `Shift-Tab` | select an annotation; then `Enter` edit label · `e` set properties · `x` delete · `c` color · `+`/`-` move step |
| `n` / `p` (or `→`/`←`) | next / previous step; step 0 shows only the code |
| `u` / `U` | undo / redo (the message names what was undone) |
| `t` | dark ⇄ light theme (like Excalidraw) |
| `P` | presentation mode (hides the UI; one geometry for the whole take — see `:layout`) |
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
:box @'// begin'..@'// end' label      range by two text markers (also :callout, :spot); quotes optional without spaces
:box 15-17 takes border=double corners=sharp fill=no   borders: border=solid|dashed|dotted|thick|double  corners=rounded|sharp  fill=yes|no (callouts take the same keys)
:hl 7:&t shared borrow                highlight (L:text must be on line L — a miss says where the text is; L: alone is the whole line)
:hl 7:&t shared emphasis=underline      how the span is drawn: fill (tint, default) · underline (a thin ‾ row under it) · brackets (⌈ ⌉) · strike — the glyphs are drawn in a softened shade of the annotation's colour, so the code reads first
:note 4 s no longer owns it           note anchored to a line
:callout 15-25 @auto title="why" two or three sentences…   bordered paragraph; @margin (default) | @below | @auto
:step 2        :step a3 2             change step · move annotation a3 to step 2
:del a1        :clear | :clear 2       delete one · clear everything or only one step (u undoes)
:label a2 new text                     change the label
:color a2 blue   :color a3 #ff8800   color per annotation (orange blue green purple pink cyan yellow red gray · auto)
:spot 15-17 a2 a3   :spot a6 a7      spotlight: in this step everything but those lines/annotations is dimmed
:redact 2:API_KEY=   :redact 3:Bearer keep=4   :redact 7   mask a secret: what follows the anchor, the last N chars shown, or the whole line (or select it and press r)
:redact auto | :redact auto --dry        opt-in: mask the shapes Tiza knows (tokens, passwords, URL credentials…) as ordinary redact annotations you can review
:redact off | :redact on | :redact      the author's own screen only; presentation mode, exports, recordings and agents always see the masks
:until s1 4                            spotlight s1 stays active until step 4
:focus 15-17   :focus                 dim everything but a range · clear the focus
:layout stable | :layout dynamic       rehearse with the take's geometry (`⊞ stable` on the bar) · let the authoring screen grow as steps add arrows
:theme light   :lang ts | :lang auto   :pad 1 4   :w   :q      (:pad = blank margins, saved in the document; :pad 0 0 none, :pad default)
:follow on | :follow off | :follow        reload the code when the source file is saved · state
:autosave off | :autosave on | :autosave  stop writing the sidecar on every change (`--no-save` at start) · resume (writes what is pending) · state
:export shot.txt | :export shot.md | :export shot.png [step=N] [width=N] [theme=light]   export what a step shows (.txt .ansi .md .svg .png)
:supervise on | :supervise off | :review   agent changes wait for your y/n (a = all, d = none); 60 s without a decision = rejected
:mouse on | :mouse off                     click moves the cursor, drag selects (then h/b/a/n…), wheel scrolls; off by default
```

The language is detected from the file extension, then well-known names (`Makefile`, `Dockerfile`, `Cargo.lock`, `.bashrc`…), then a `#!` line, then a Vim/Emacs modeline in the first or last five lines; pasted or piped code uses the same shebang/modeline rule. `--lang` and `:lang <id>` override it (saved in the document); `:lang auto` re-detects and says how; an id without a grammar is shown as plain text with a warning.

New annotations appear at the current step. The `.tiza` references the source file by path (`"code": {"file": "main.rs", "lines": "12-48"}`, absolute or relative to the `.tiza`), so it always shows the current code; `:embed` freezes a copy inside the document and `:link path[:A-B]` references a file again. Color is yours, as in Excalidraw: every annotation accepts `color` (a name or `#rrggbb`); arrows without a color rotate through a palette so they can be told apart. Arrow strokes and heads are yours too: dashed for a shared borrow, thick for a move, `heads=none,none` for a plain connector — the ownership example does exactly that. Everything is saved automatically to a `.tiza` (JSON) next to the source file (`src/main.rs.tiza`).

**Follow mode** (`--follow` or `:follow on`) watches the source file's folder (Zed saves with a temp file + rename) and reloads the code about 120 ms after the last write, so you can edit in the editor while the TUI shows the result. Annotations are re-anchored by the text they point at, like a paste: a line diff matches the two versions in order (so a note on the second `}` stays on the second `}`), then the same occurrence of the span inside the line; a block moved elsewhere is followed when its line is unique and within 50 lines (a callout on it travels with it), a box, callout or spotlight whose edge line you edited or moved away keeps its height from the edge that stayed (it never stretches), a line you rewrote in place keeps its note and the spans whose text is still there (also when several adjacent lines are rewritten together, like a parameter renamed on the signature and on the line that uses it: the lines pair up one to one), a formatter that splits or joins a line (rustfmt) carries the span to the line that now holds its text, and the ones whose text is gone are dropped with a message — never moved to another copy — and `u` undoes the whole reload. A reload that changes no annotation leaves no undo step, so after an editor undo one `u` brings the dropped annotations back; the message after `u` names what it undid (`undone: reload #54`, `undone: note a3`, `undone: 3 annotations cleared`). The status line shows `⟳ following · last HH:MM:SSZ`; a removed file shows `waiting` until it comes back, an empty intermediate write is skipped, and embedded documents have nothing to follow (`:link path` first). Agents get the same through `follow.set` / `follow.status` and the MCP tool `set_follow`.

**Saving** — every change is written to the `.tiza` sidecar as it happens (an atomic replace next to the source). Three things turn that off: `tiza FILE --no-save` (annotate an example or a demo without touching the file on disk — the bar shows `no-save`, then `unsaved (:w)` once something changed), `:autosave off` live (`:autosave on` resumes and writes what is pending; `:autosave` alone shows the state, and `status` carries `autosave`), and a `scene put` of a document from another source (the agent's document is shown, but the file that was open is never overwritten by it; the message says so). With autosave off, `:w` saves to the open file on purpose, `:w PATH` saves elsewhere, and `:q` refuses while there are unsaved changes (`:q!` discards). A sidecar that cannot be written (a read-only file or folder — the atomic replace honours the file's read-only bit instead of walking over it) warns once — `read-only: could not save … ; :w PATH saves elsewhere` — and the TUI keeps working with the short form on the bar's right; `render`, `scene get`, the exports and `--smoke` never write.

**Moves** (the code changes between steps, [ADR 0007](docs/adr/0007-moves-y-pasos-con-nombre.md)): a `.tiza` may carry `moves`, each a `code.patch` that inserts, replaces or deletes whole lines when the presenter reaches `at_step` — `n` applies it, `p` reverts it, and the document is always recomputed from the step-0 code, so going back and forth ten times leaves it exactly as it started. A target is matched on the code as shown at that step and must match exactly once (`after`/`before` a line's text or `position` for an insert; `find`, `find_start`/`find_end` or `position: "7-9"` for a replace or delete) — or say which match with `"occurrence": N`, 1-based and counted in reading order over the whole code at that step (`end_occurrence` for `find_end`). With `"typing": "line"` or `"char"` the content is typed in place (`speed_ms` per line or character; any key finishes it and is consumed — an `n` pressed mid-typing completes the step instead of advancing, so press it again; the step's annotations appear when the typing ends). Annotations are authored against the code at their own step and shifted through later patches by the patch itself — an insert or delete moves the lines after it, a box that loses an edge line shrinks — never by a text search. On a line a `replace` overwrites, a note, box, callout or spotlight keeps the line at the same offset, but a highlight, mark or arrow end has nothing under its columns any more and is hidden as `(unanchored)` in `:list` (never deleted) unless the move's `rebind` says where it goes (`"rebind": {"a4": {"at": {"find": "t.len()", "occurrence": 1}}}`, resolved on the code after the patch); the same happens to any annotation that loses every line. `tiza examples/ownership-moves.rs.tiza` shows a helper being typed in and later replaced by a method call; `render --step N`, `.cast` and `.tape` apply the moves instantly (`render --step N` past the last step warns on stderr and shows the last one; a `.tape` holds a typed step for the typing time plus half a second). Paste and follow are refused while a move is applied (step back to 0 first); at step 0 a paste re-applies the moves on the new text and re-anchors every annotation on the code at its own step, so a step-3 arrow onto a line a move typed in survives; `:embed` and the saved file always keep the step-0 code.

## Export to text, Markdown, ANSI, SVG, PNG, GIF, MP4, asciicast or VHS

```sh
tiza render demo.tiza --out thumb.png --step 3          # PNG at 2× (thumbnails, posts)
tiza render demo.tiza --out figure.svg --theme light     # SVG with the fonts embedded
tiza render --out shot.png                                   # what the live instance shows right now
tiza render demo.tiza --out notes.md --step 2            # fenced code block for a README, an issue or a chat
tiza render demo.tiza --out - --format ansi | less -R    # 24-bit ANSI, no trailing spaces
tiza render demo.tiza --out demo.gif                     # animated GIF of every step, drawn by Tiza (nothing to install)
tiza render demo.tiza --out demo.mp4 --interval 3        # MP4 through the ffmpeg on your PATH (never bundled)
tiza render demo.tiza --out demo.cast                    # asciicast v3 of every step (asciinema, agg)
tiza render demo.tiza --out demo.tape && vhs demo.tape   # VHS tape → demo.gif, for the terminal-window look
```

**Stable geometry.** A take is one layout, not seven: in presentation mode and in every export (`render --step N` for each step, `.gif`, `.mp4`, `.cast`, `.tape`) the arrow lanes, the box band, the code column and the right margin are sized once for the maximum over every step of the document, so the code never slides sideways when a later step adds an arrow and a label that fits at step 1 still has its margin at step 7 (each arrow also keeps its lane for the whole take). The authoring screen grows as you go by default; `:layout stable` rehearses with the take's geometry (`⊞ stable` on the status line) and `:layout dynamic` goes back.

**Recording every step.** Four outputs turn the whole take into an animation, every one drawn from the same frames as the PNG export (one geometry for the take, masks on), so nothing slides between steps and nothing shows that the screen does not:

- `--out demo.gif` — an animated GIF written by Tiza itself, nothing to install: one frame per step from 0 (`--skip-zero` starts at the first annotated step), `--interval` seconds each (2; the last one twice as long so a loop does not cut the end short), the typed frames of `code.patch` moves at their `speed_ms` (the format counts in 10 ms units and players treat anything under 20 ms as 100 ms, so faster typing is rounded up to 20 ms), looping forever (`--loop N` plays it N times), `--scale` 1 by default (2 for a 2× video frame), one palette per animation (the colours that cover most pixels — background, code, rails — kept exactly, the anti-aliasing blends reduced into the rest of the 256, no dithering so text stays crisp) and every frame after the first stored as the rectangle that changed: the seven-step `examples/ownership.rs.tiza` is ~170 kB at 100 columns. The summary line says frames, size, seconds per loop and colours.
- `--out demo.mp4` (or `.webm`) — the same frames muxed by **`ffmpeg` from your PATH** (or `$TIZA_FFMPEG`): Tiza writes them to a temporary folder with their durations and runs a fixed command it prints first (30 fps, even dimensions, `yuv420p` H.264 for MP4, VP9 for WebM), then removes the folder. ffmpeg is never bundled, downloaded or vendored, not even behind a feature flag ([ADR 0009](docs/adr/0009-politica-de-red.md) spirit: the binary that shows your code stays what it is); without it the command exits with code 2, says how to install it (`brew install ffmpeg`, `apt install ffmpeg`…) and reminds you that `.gif` and `.cast` need nothing. No half-written file is ever left behind (temp + rename).
- `--out demo.cast` — an [asciicast v3](https://docs.asciinema.org/manual/asciicast/v3/) of every step as text (tiny; play it with `asciinema play demo.cast`, embed it with the asciinema web player, or turn it into a GIF with [agg](https://github.com/asciinema/agg)).
- `--out demo.tape` — a [VHS](https://github.com/charmbracelet/vhs) tape that runs the real TUI in presentation mode and presses `n` through the steps, for the VHS look (a terminal window with its chrome): `vhs demo.tape` needs `vhs`, `ttyd` and `ffmpeg` and produces `demo.gif`. The tape sizes the window for the TUI at the given width (12 px per column and two spare rows at `FontSize 18`, plus the padding), colours it with the document theme (`--theme light` for a light window), quotes every path and holds a typed step for the typing time plus half a second.

All four start at step 0 (code only) unless `--skip-zero`, honour `--interval`, and after writing, the CLI says how to play the file.

The text formats keep the box-drawing characters as they are on screen (`.txt`: plain, no trailing spaces or blank rows; `.md`: the same inside a ```` ```text ```` block with the document title above; `.ansi`: 24-bit colors with a reset per line, for `cat`, `less -R` or a terminal-recording tool). Inside the TUI, `:export shot.md` (or `.txt`, `.ansi`, `.svg`, `.png`, with `step=`, `width=`, `theme=`) writes the same next to the document.

Options: `--width` (columns, default 100 or the live terminal's), `--font-size` (16 px), `--margin` (1.5 cells), `--radius` (12 px, `0` for square corners), `--scale` (PNG, 2), `--no-embed-fonts` (small SVG that uses the viewer's fonts), `--system-fonts` (PNG fallback for emoji/CJK), `--brand-font` (notes, labels and legends in *Tiza Serif*, the brand typeface, instead of the monospace; `--mono-labels`, the old switch, is a no-op since the monospace became the default). The fonts ship inside the binary — JetBrains Mono (OFL) for the grid and, by default, for the prose too, a symbols subset of DejaVu Sans Mono, and *Tiza Serif*, a Latin subset of [Libron](https://github.com/nicoverbruggen/libron) (OFL) — so the output is identical on every machine. Agents get the same through the `export_image` MCP tool (which also writes `.txt`, `.ansi` and `.md`).

![ownership example, step 7](docs/img/ownership-step7.png)

The GIF at the top of this README is eight of these exports (`--all-steps` is t-101; today it is a loop over `--step`) stitched with Pillow — no screen recording involved.

## For agents: MCP and local API

With the TUI open, start the MCP server in another process:

```sh
tiza mcp
```

Configuration for Claude Code (the project's `.mcp.json` or `claude mcp add tiza -- tiza mcp`; if you had the server registered under the old name `codedraw`, remove it, add `tiza` and reconnect with `/mcp`):

```json
{ "mcpServers": { "tiza": { "command": "tiza", "args": ["mcp"] } } }
```

Tools: `status`, `get_code`, `list_annotations`, `add_mark`, `define_style`, `list_kinds`, `add_arrow`, `add_box`, `add_note`, `add_callout`, `highlight`, `add_spotlight`, `update_annotation`, `delete_annotation`, `clear_annotations`, `add_move`, `delete_move`, `list_moves`, `set_step`, `next_step`, `prev_step`, `set_focus`, `set_theme`, `set_follow`, `render_text`, `export_image`. Anchors accept `{"find": "takes(s)"}` in addition to line and column, because an agent rarely knows the exact column; boxes, callouts and spotlights also take `find_start`/`find_end` (two texts that delimit the range, like Demo Time placeholders) instead of line numbers; `update_annotation` moves an existing annotation with the same anchors, keeping its kind and style. The server also publishes four MCP resources a client can attach as context without a tool call: `tiza://scene` (the document as JSON), `tiza://code` (numbered code), `tiza://render` (the current frame as text) and `tiza://render.md` (the frame in a Markdown fence). Full guide: [docs/agent-guide.md](docs/agent-guide.md).

**Mouse** (`--mouse`, `:mouse on|off`): off by default, because capturing the mouse takes the terminal's own text selection away; while it is on, a click moves the cursor, a drag selects a span (one line) or a range of lines and leaves the selection open for `h` (highlight), `b` (box), `a` (arrow end), `n` (note), `C` (callout), and the wheel scrolls. It is for live, spontaneous explanations in normal mode, where the status bar shows what you select; presentation mode (`P`) ignores clicks, drags and the wheel (the cursor and the selection are hidden there, so a mouse selection would be blind) — to annotate with the mouse, leave presentation mode first. To copy text from the terminal while Tiza captures the mouse, hold Shift while dragging (Ghostty, WezTerm, most Linux terminals) or Option (iTerm2, Terminal.app); Zed keeps Cmd+click for links even with mouse reporting on; under tmux the mouse reaches Tiza only with `set -g mouse on`.

**Supervised mode** (`:supervise on`, socket `supervise.set`): while you present, every change an agent asks for is parked as a proposal — the review overlay lists them (`▶ p1  add note L4 «…»`), the status bar shows `⏸ N to review (y/n)`, and the agent's call only returns once you press `y` (applied, normal result) or `n` (error `rejected`); `a`/`d` decide everything at once, Esc hides the list and `:review` brings it back, and a proposal nobody decides within 60 s is rejected. While the list is open, Enter accepts the first proposal; `:` hides it and opens the command line, so typing a command from the list is safe. In presentation mode the list does not pop up: a chip `⏸ N to review` shows bottom-left instead, and the list opens by itself when you leave presentation mode (`P`) with proposals pending. Reads (`status`, `get_code`, `render_text`, the resources) never wait.

**After rebuilding Tiza** (`git pull && cargo build`, `cargo install`): an MCP client reads the tool list once per connection and keeps talking to the `tiza mcp` process it started, which is still the old binary. New or changed tools appear only after the client reconnects — in Claude Code: `/mcp` → `tiza` → *Reconnect* (or restart the session). The `status` tool says when this is needed: its `bridge` object carries the server's `version` and `build` (git commit, also in `tiza --version`) and sets `binary_rebuilt: true` with a hint once the binary on disk is newer than the running server, or `instance_build` when the TUI was built from another commit (restart the TUI too). `notifications/tools/list_changed` would not help here: the stale process cannot know the new one's tools.

The contract is published as JSON Schema: [`schema/tiza.schema.json`](schema/tiza.schema.json) (the document) and [`schema/api.schema.json`](schema/api.schema.json) (every socket method, params and result), regenerated with `tiza schema document|api` and checked in CI; the live instance also answers `api.schema`. `-` means stdin everywhere: `cat demo.tiza | tiza render - --out -`, `pbpaste | tiza render - --lang rs --out shot.png`, `tiza api annotation.add -`.

The same API is available as a Unix socket with line-delimited JSON (design borrowed from herdr) and from the CLI:

```sh
tiza scene status                 # is there an instance? what is it showing?
tiza scene list                   # annotations
tiza scene text                   # what is on screen, as text
tiza api annotation.add '{"type":"note","line":4,"text":"hello"}'
tiza scene put other.tiza      # replace the live document (u undoes, step included); also takes a `scene get` envelope; a plain file only with --as-code
tiza scene patch '{"step":2,"theme":"light","annotations":[{"id":"a1","deleted":true}]}'   # partial change, merged by id
tiza api move.add '{"at_step":2,"action":"code.patch","op":"insert","after":"let b = 2;","content":"    let c = 3;","typing":"line"}'   # the code changes at step 2
tiza render demo.tiza --out - --step 2 --width 100   # without opening the TUI
```

Several instances: `--session name` (or `TIZA_SESSION`). Explicit socket: `--socket path` (or `TIZA_SOCKET`). The socket lives in `$XDG_RUNTIME_DIR` or `$TMPDIR` with 0600 permissions and **never** listens on TCP.

## Redaction: masking secrets on every surface at once

A `.env` with a real token, a URL with credentials, a password in a YAML: Tiza owns the buffer it shows, so one annotation masks the span everywhere — the screen (normal and presentation mode), every export (`txt`, `md`, `ansi`, `svg`, `png`), the `.cast` and `.tape` recordings, `render.text`, and what agents read through `get_code`, `scene.get` and the `tiza://` resources. The file on disk never changes, and the `.tiza` stores only the columns of the mask, never the text under it (`:embed` freezes the masked code and says so). Two flows, both yours:

- **By hand.** `:redact 2:API_KEY=` masks what follows `API_KEY=` to the end of line 2 (the anchor is the text *before* the secret — typing the secret itself into the command line is refused); `:redact 3:Bearer keep=4` leaves the last four characters visible (`••••••••a1b2`); `:redact 7` masks the whole line; `:redact 2@8..31` masks columns; select the value with `v` (or the mouse) and press `r`. Agents: `redact {at: {line, find: "API_KEY="}}` or `{at: {line, col, end}}`. The mask is always eight `•` whatever the secret's length; `:list` shows `redact L2 (masked)` with the columns, `:del ID` removes one, `u` undoes.
- **`:redact auto`, opt-in, never on by itself.** It scans the shown code for the shapes it knows — JWTs, `Bearer`/`Basic` tokens, `password` / `secret` / `token` / `api_key` values in env, JSON, YAML, TOML and INI, credentials inside URLs, AWS / GitHub / Stripe / Slack / Google key prefixes, e-mail addresses — and turns every match into an ordinary `redact` annotation, so you see exactly what was masked, delete what you do not want and add what it missed. `:redact auto --dry` only lists the matches. It reports `redact auto: 3 masked (L2 env, L3 jwt, L4 url-credentials)` and `0 matches` when nothing fits.

Masks are active from step 0 whatever the annotation's step (a secret is never visible at an earlier step; `"redact_from_step": true` in the document changes that), the status line shows `◆ redacted`, and `:redact off` lets *you* see the real text on your own screen while agents, exports and presentation mode keep the mask (the bar then shouts `◆ UNMASKED`). A text search never confirms what is under a mask: `find` from an agent or a move on a masked substring fails like text that is not there. The limits: the source file still contains the value; a secret wrapped over two lines needs two redacts; `auto` only knows the listed shapes — read the screen before you record. If follow mode or a paste changes a masked line so the mask cannot follow, Tiza drops it with a loud notice (`redact a3 dropped: the line changed — check the screen before recording`).

## What Tiza reads and writes

**The `tiza` binary that shows your code has no network dependencies and never opens a connection.** Its only I/O is your terminal, the files you name, the `.tiza` sidecar next to them, and a local Unix socket for agents (never TCP). Features that talk to the internet live in a separate crate and binary, are off unless you run them, and state what they send before they send it ([ADR 0009](docs/adr/0009-politica-de-red.md)).

In detail, `tiza` reads the source file you open (and re-reads it in follow mode), the `.tiza` document, the files a `.tiza` references, and what agents send over the socket; it writes the `.tiza` sidecar (every change, unless `--no-save` / `:autosave off`), the exports you ask for (`:export`, `render`, `export_image`) and the socket itself. It never reads your shell history, your editor's configuration, environment variables other than `TIZA_*`, `TMPDIR` and `XDG_RUNTIME_DIR`, or any file you did not name. Redaction (`:redact`, `:redact auto`) runs locally with regexes over the code it already shows; nothing is sent anywhere to decide what a secret is. There is no telemetry, no crash reporting, no update check and no identifier of any kind; `Cargo.lock` has no HTTP or TLS crate, and `cargo-deny` keeps it that way (`deny.toml`). When a network feature exists it will be a separate `tiza-net` binary that runs only on your command (`tiza-net publish demo.gif`, `tiza-net update-check`), prints what it is about to send, never includes your code or annotations unless sending them is the point and you asked, and applies the redaction masks first.

## Zed integration

A task that sends the current selection to Tiza in the integrated terminal: [docs/zed.md](docs/zed.md).

## Recommended recording profile

Full-screen terminal or 50 % next to Zed, 18–22 px font, snippets of ≤ 60 columns. The status line shows `cols×rows` and a ⚠ badge when the terminal is under 100×30 or a code line is clipped (`status` over the socket lists the same `warnings`); below ~100 columns, notes are drawn under the line instead of in the margin (an arrow's label then sits on the arrowhead's row right after the code when it fits, or on the row below behind the arrow's own glyph — `─▶ move` — so it reads as the arrow's, never as a loose comment), and callouts with `placement: auto` move below their lines. One blank column is kept at each side by default (`:pad V H` to change it, `:pad 0 0` to remove it, `:pad default` to go back). Both themes use explicit RGB colors designed for 1080p and checked after YouTube-like compression (`scripts/legibility.py`, run in CI: every text color keeps ≥ 4.5:1 against the background). The terminal font is yours: JetBrains Mono (the font the exports use for code) or any humanist monospace pairs well with the serif notes of the exported stills.

## Development

```sh
cargo test                                   # core, TUI (incl. socket), commands and the per-ticket QA tests (qa/)
cargo clippy --all-targets -- -D warnings
cargo run -- examples/ownership.rs
```

Crates: `tiza-core` (model, anchors, layout, rendering, protocol; no terminal dependencies), `tiza-tui` (ratatui + crossterm + socket server), `tiza-mcp` (MCP bridge with [rmcp](https://github.com/modelcontextprotocol/rust-sdk)), `tiza-export` (SVG/PNG with [resvg](https://github.com/linebender/resvg) and embedded fonts), `tiza` (the binary, in `crates/tiza-cli`). Decisions in [docs/adr](docs/adr). Dependency policy (maintained crates only, `cargo-deny` in CI): [docs/adr/0004-dependencias.md](docs/adr/0004-dependencias.md).

## Name

There is a Java library called [Tiza](https://github.com/Krassnig/Tiza) (drawing for beginners). They are unrelated; this project is a Rust terminal tool.

## License

MIT or Apache-2.0, at your option.
