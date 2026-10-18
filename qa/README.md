# QA tests, one file per board task

`qa/tests/t-XXX.rs` is the automated side of the card **t-XXX** on the board: its header repeats the task's acceptance criteria and each `#[test]` is one of the card's test steps or corner cases, named `acN_…`. Cargo compiles every file here as an independent crate, so one ticket's test can be run alone:

```sh
cargo test -p tiza-qa                 # every ticket
cargo test -p tiza-qa --test t-092    # one ticket
cargo test -p tiza-qa --test t-050 -- ac3 --nocapture
```

Test data lives in `qa/fixtures/t-XXX/` (never in `examples/`, which are the demos people open). `tiza_qa` (in `src/lib.rs`) gives the helpers: `bin()` builds and locates the `tiza` binary, `tiza_ok(&[…])` runs it, `fixture("t-092/callouts.tiza")`, `scratch(name)` for output, and `Live` — a headless instance with the real `App` and the real socket server, so a test can call the API (`live.api_ok("annotation.add", json!({…}))`), press keys (`live.keys(&[KeyCode::Char('V')])`), run `:` commands and read the screen (`live.screen(100, 30)`).

Tests that need a real terminal use `tiza_qa::pty_script(dir, name, body, args)`: it prepends `src/pty.py` to the test's Python body and runs it with a hard wall-clock timeout on the Rust side. The prelude's `Pty` class never blocks: a reader thread drains the pty through `select` from the start (so the child never blocks writing while the body runs `tiza api` or anything else; t-157), `wait_for(pred, timeout, what)` waits on what the child printed (never `time.sleep`), `finish()` kills a child that does not exit, and a SIGALRM backstop ends the script at its deadline. A child that stops answering fails the test in seconds instead of hanging `cargo test` (t-119). Never write a bare `os.read` loop on a pty or call `Command::output()` on an interactive child.

`McpClient` (in `src/lib.rs`) is a real JSON-RPC client over stdio for `tiza mcp`: handshake, `tools/list`, one `tools/call` at a time with the reply read in order, so reconnection and errors can be observed (t-070).

Render snapshots (t-053) live in `qa/tests/snapshots/` (insta): one `-symbols` and one `-styles` file per case. When a render changes on purpose, run `INSTA_UPDATE=always cargo test -p tiza-qa --test t-053` (or `cargo insta review`) and commit the updated `.snap` files with the change.

When a ticket is done, its card gets: acceptance criteria, manual test steps, corner cases and the path of this file (`qa_test`). Keep the three in sync: if a corner case is added to the card, add the test here.

Property tests (`t-074`) keep their failing seeds in `qa/tests/t-074.proptest-regressions`; commit that file when proptest adds a seed so the case is replayed first on every run.

`scripts/legibility.py` (t-072) is the YouTube-compression check: it renders a step, scales it to 1080p, round-trips it through H.264 with ffmpeg and measures the contrast every text color keeps. `qa/tests/t-072.rs` and the Mocha step of `qa/tests/t-119.rs` run it when python3 + Pillow + numpy + ffmpeg are installed (shared check `tiza_qa::legibility_tools_missing()`; otherwise they skip and print what is missing) and CI runs it in the `legibility` job (report and compressed frames as an artifact).

## Terminal size matrix with Herdr (t-071)

A QA-round step on a machine with [Herdr](https://herdr.dev) (Luis's Mac), not part of `cargo test`: Herdr renders through libghostty-vt, a real terminal emulator, so it catches escape-sequence and redraw problems the pty snapshots (t-053, t-064, t-073) cannot. From a Herdr pane at the repository root:

```sh
scripts/herdr-matrix.sh            # capture into qa/herdr/latest/ (git-ignored), print each pane's size
scripts/herdr-matrix.sh --check    # capture and diff with qa/herdr/*.txt; exit 1 and list the panes that changed
scripts/herdr-matrix.sh --update   # capture and rewrite qa/herdr/*.txt on purpose (commit them with the change)
scripts/herdr-matrix.sh --check --file /tmp/copy.rs.tiza   # another document (default examples/ownership.rs.tiza, only read)
```

For each case — `vt100` 80×24, `wide` 120×40, `zed-half` 75×50, `narrow` 60×30 and `full` (as large as the base pane allows, about 12 columns left to it), all `--present --step 7`, plus `normal` 120×40 and `normal-narrow` 60×30 in normal mode with the status bar (the narrow one shows the small-terminal badge, t-064) — it reads the base pane's size from `herdr pane layout` (each pane's `rect.width`/`rect.height`) and splits one pane off it already at the target width, `herdr pane split BASE --direction right --ratio R` with R = 1 − target_cols/base_width (the ratio is the share the base keeps: on 202 columns R 0.604 gives 80), then splits that pane the same way with `--direction down` for a row target. A pane that misses by a cell or so is fixed with `herdr pane resize`, always in the direction that grows the pane resized: towards the base (`left`/`up`) on our pane to grow it, towards our pane (`right`/`down`) on its neighbour to shrink it — Herdr does not move an edge a pane does not share, so `resize right` on a pane at the right does nothing. Then it runs Tiza there with `--no-save` and a socket of its own (the size `tiza api status` reports is printed next to Herdr's as a cross-check, with a warning if they differ), waits for the instance to answer and for two identical `herdr pane read --source visible --format text`, and saves `qa/herdr/<case>-<cols>x<rows>.txt` with the **real** size: a pane that misses its target, or a different window, shows up in `--check` as a size change rather than as a broken frame. Herdr 0.9.5 has no `pane wait-output` and no command to open a tab, so the cases run one after another off the base pane (`--base PANE_ID`, default Herdr's focused pane) and every pane the script opened is closed again, also on an error or Ctrl-C; the user's own panes are only split and given their space back. Without `herdr` on PATH, or with a Herdr that does not answer `herdr pane list`, it stops with a message before opening anything.

Every `herdr` call goes through one function; with `TIZA_HERDR_FAKE=<state dir>` it is a model instead (panes are Python ptys of the modelled size running the real `tiza`, `pane layout` gives their rects, a pane never shrinks by resizing it away from the edge it shares, and `pane read` is that instance's `tiza render`), which is how `qa/tests/t-071.rs` checks the rest of the script — sizes, file names, `--check`/`--update`, cleanup on failure — without Herdr. The frames in `qa/herdr/` come only from a real Herdr run.

## Size breakdown of the first baseline (t-133)

Where the bytes of the release binary go, next to the baselines in `qa/budgets.json` and `qa/bench/baseline.json`. Measured on 2026-10-18 on Linux x86_64 at 5e57795 (fat LTO, the t-133 profile; the committed Linux baseline at 5ab5bb1 is 13,158,152 bytes, this tree is 13,171,048 stripped, +0.1 %) with `cargo-bloat 0.12.1`: `CARGO_PROFILE_RELEASE_STRIP=false cargo bloat --release -p tiza --bin tiza --crates -n 0` for the code, `size -A` on the stripped binary for the sections, and a byte search in the stripped binary for each embedded blob. The Mac binary (11,209,360 bytes) was not broken down; Mach-O has smaller unwind and relocation tables, so expect the same proportions with a smaller "other sections" row. cargo-bloat attributes code by symbol name: generic code lands in the crate that instantiates it (most of ratatui and serde shows up under `tiza_*` and `serde_core`), so the per-crate code rows are approximate.

| Part of the 13.17 MB stripped binary | Bytes | Share |
|---|---:|---:|
| Code (`.text`) | 8,524,231 | 65 % |
| Read-only data (`.rodata`) | 3,069,072 | 23 % |
| Unwind tables (`.eh_frame`, `.eh_frame_hdr`, `.gcc_except_table`) | 937,652 | 7 % |
| Relocations and vtables (`.data.rel.ro`, `.rela.dyn`) | 623,984 | 5 % |

Code by area (cargo-bloat, `.text` only):

| Area | Crates | Bytes |
|---|---|---:|
| SVG / PNG / GIF export and text shaping | usvg 420 K, harfrust 408 K, skrifa 245 K, tiny-skia 196 K, read-fonts 141 K, resvg 80 K, roxmltree 80 K, fontconfig-parser 67 K, fontdb 57 K, miniz_oxide 44 K, … | 1,885,041 |
| Tiza's own crates | tiza-core 749 K, tiza-tui 418 K, tiza-mcp 268 K, tiza (cli) 206 K, tiza-export 56 K | 1,696,378 |
| Rust standard library | std / core / alloc | 1,473,288 |
| Serialization | serde_core 524 K, serde_json 366 K, schemars 198 K, serde 106 K | 1,194,155 |
| Regex engines behind syntect's `regex-fancy` | regex-automata 373 K, regex-syntax 174 K, aho-corasick 140 K, fancy-regex 54 K, … | 775,168 |
| MCP server and async runtime | rmcp 539 K, tokio 132 K, futures | 685,147 |
| CLI parsing | clap_builder 257 K, … | 269,591 |
| TUI (as attributed) | ratatui, crossterm, unicode-width, … | 124,523 |
| Syntax highlighting code | syntect 59 K, two-face, bincode | 86,262 |
| Everything else (35 crates) | toml 62 K, notify 42 K, toml_parser 27 K, anyhow 17 K, … | 268,677 |

Embedded data inside `.rodata` (found byte for byte in the stripped binary):

| Blob | Bytes |
|---|---:|
| two-face syntax set (`syntaxes-fancy-newlines.bin`, bat's grammars) | 959,011 |
| JetBrains Mono Regular / Bold / Italic / BoldItalic (SVG/PNG export) | 1,108,400 |
| TizaSymbols.ttf (symbols fallback) | 125,188 |
| two-face themes (`themes.bin`) | 62,593 |
| Rest of `.rodata` (strings, Unicode and font tables, constants) | ~813,880 |

Not linked (checked): syntect's own default dumps, two-face's `no-newlines` and onig dumps, two-face's acknowledgements. Reading the table: about 3.1 MB (24 %) is export (shaping + rasterizing code and the five fonts), about 1.9 MB is highlighting (two-face grammars and themes + the regex engines + syntect), and 1.9 MB is MCP + serialization. `panic = "abort"` would remove most of the unwind tables (~0.9 MB here, ~1.6 MB measured on the Mac) and was discarded on purpose (see `[profile.release]` in `Cargo.toml`).

## Checklist for any card that adds or changes a `:` command (t-115)

A typo must never close the app, leave the terminal broken or create something other than what was typed. Every rejection ends in a line that says how to go on (the expected shape, the valid values, or `:help`), and a success message appears only when the annotation is exactly what the user asked for. `qa/tests/t-115.rs` drives this table through the real command runner; add the new command's own rows there.

| Typed | Expected |
|---|---|
| `:box 3 - 5 x`, `:box -1-3 x`, `:box 0-0 x` | usage line; the first one adds «write A-B without spaces»; the last `line 0 does not exist` |
| `:hl 3 s`, `:hl s`, `:mark warn s` | usage (`L:text`; `L:` alone is the whole line) — a missing colon or line never anchors anything |
| `:hl 3:S x`, `:mark warn 1:takes` | `«S» not on L3; found on L2 — use 2:S` (never an annotation on a line the user did not name) |
| `:hl 999:x y`, `:arrow 1:s 999:s x` | `line 999 does not exist (the document spans 1 to N)` |
| `:focus 0`, `:focus 99-100` | `line … does not exist`; `:focus 2-99` is clamped with a notice |
| `:until s1 0`, `:step a1 99999`, `:step a1 0` | error naming the bound (the spotlight's step, `last step + 1`, `steps ≥ 1`) |
| `:set a1 =`, `:set a1 color=`, `:style io glyph=` | `expected key=value, got «…»` |
| `:lang rs rs rs`, `:callout 7-9 @bogus txt` | usage / `unknown placement @bogus: use @below, @margin or @auto` |
| `:bx`, `:stpe`, `:colr` | `unknown command: bx — did you mean :box?` (ties list up to three) |
| `:export x.png width=99999`, `tiza render --width 99999`, `--scale 99` | `width=99999: use a number between 20 and 400`, `scale=99: use a number between 0.25 and 8`; a PNG over 16384 px per side or 100 MP is refused naming the limit |
| any panic (`TIZA_TEST_PANIC=1` + `:panic` in a test) | the terminal is restored and one `tiza crashed: … Your work is in FILE.tiza … issues` line is printed |
| valid but odd input: emoji, RTL marks, tabs, `%s%n`, quotes, `$(id)`, a 20 000-character label | keeps working |
