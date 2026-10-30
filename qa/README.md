# QA tests, one crate per area

`qa/tests/<area>/main.rs` is one test crate. It has one module for each behavior of that area, and each `#[test]` in it covers something that can break, or a bug that someone fixed. The name of a test tells the behavior. A step that only checks text stays a manual step on the card. [AGENTS.md](../AGENTS.md) gives the rule.

The areas are `render`, `tui`, `follow`, `api`, `steps`, `export`, `saving`, `check`, `mcp`, `release` and `policy`. Four crates hold tests that must not share their process with others: `env` (tests that change the environment of their own process, one at a time), `latency` (tests that time a child process), `cache` (tests that check the highlight cache of the whole process) and `watchers` (tests that run a TUI with follow on a pty).

```sh
cargo test -p tiza-qa                              # every area
cargo test -p tiza-qa --test render                # one area
cargo test -p tiza-qa --test render line_numbers   # one module
cargo test -p tiza-qa --test export -- svg_and_png::svg_is_standalone_and_fonts_are_optional --nocapture
```

Test data lives in `qa/fixtures/<area>/<module>/`, and data that several areas use in `qa/fixtures/shared/` (never in `examples/`, which are the demos people open). `tiza_qa` (in `src/lib.rs`) gives the helpers: `bin()` builds and locates the `tiza` binary, `tiza_ok(&[…])` runs it, `fixture("render/callouts/callouts.tiza")`, `scratch(name)` for output (a new empty folder on each call, also with the same name), and `Live` — a headless instance with the real `App` and the real socket server, so a test can call the API (`live.api_ok("annotation.add", json!({…}))`), press keys (`live.keys(&[KeyCode::Char('V')])`), run `:` commands and read the screen (`live.screen(100, 30)`).

A helper that several areas use goes in `tiza_qa`. A helper that several modules of one area use goes in `tests/<area>/common.rs`, which that area's `main.rs` declares with `mod common;` (as in [Submodules in integration tests](https://doc.rust-lang.org/book/ch11-03-test-organization.html#submodules-in-integration-tests)). Do not copy a helper.

Tests that need a real terminal use `tiza_qa::pty_script(dir, name, body, args)`: it prepends `src/pty.py` to the test's Python body and runs it with a hard wall-clock timeout on the Rust side. The prelude's `Pty` class never blocks: a reader thread drains the pty through `select` from the start (so the child never blocks writing while the body runs `tiza api` or anything else), `wait_for(pred, timeout, what)` waits on what the child printed (never `time.sleep`), `finish()` kills a child that does not exit, and a SIGALRM backstop ends the script at its deadline. A child that stops answering fails the test in seconds instead of hanging `cargo test`. Never write a bare `os.read` loop on a pty or call `Command::output()` on an interactive child.

`bin()` builds `tiza` with the `test-hooks` feature: `TIZA_TEST_HOOK=request:<method>` makes the TUI panic after a request did its work, `TIZA_TEST_HOOK=effect:slow` makes the clock of an enter effect 100 times slower, `effect:late-frames` does the same and makes each frame of the effect 50 ms longer, `draw:slow-first-frame` makes the first frame of the TUI 1.5 s longer and `draw:slow-frames` makes each frame 100 ms longer, `TIZA_MCP_TEST_HOOK` (`tool:<name>`, `export:render`, `export:slow-write`) does the same in the MCP server, and `TIZA_MCP_EXPORT_TIMEOUT_MS` shortens the export deadline; a release build has none of them.

`TIZA_QA_BIN=<path>` makes `bin()` use that binary and not build `tiza`. Use it only for a load run: many test processes at the same time on one `target`. On macOS each `cargo build` removes `target/debug/tiza` and copies it again, also when nothing changed. Then a test of another process cannot start the binary. The file must be a build of this tree with `--features test-hooks`. Unset the variable before the checks of [AGENTS.md](../AGENTS.md). When `CI` is set too, the tests fail.

`Live` does not wait for a follow watcher inside a request, as the TUI does not. `api`, `command` and `keys` wait after the request until a watcher that is starting runs, for 30 s at most (`FOLLOW_START_BUDGET`). On macOS a start takes milliseconds, but fseventsd reads the folder of the test binary each time a watcher starts. A `target/debug/deps` with a million old `.o` files makes each start take about 2 s, and the tests in parallel wait for each other's starts. When a test fails with «the follow watcher did not start», run `cargo clean`.

`McpClient` (in `src/lib.rs`) is a real JSON-RPC client over stdio for `tiza mcp`: handshake, `tools/list`, one `tools/call` at a time with the reply read in order, so reconnection and errors can be observed. Replies are read on a thread of their own and waited for with a deadline (`request` fails the test after 120 s, `request_within` returns `None`), so a call the server never answers fails in seconds instead of hanging `cargo test`.

Render snapshots live in `qa/tests/render/snapshots/` (insta), with the module path in their names: one `-symbols` and one `-styles` file per case. The other areas keep theirs in `qa/tests/<area>/snapshots/`. When a render changes on purpose, run `INSTA_UPDATE=always cargo test -p tiza-qa --test render frame_snapshots` (or `cargo insta review`) and commit the updated `.snap` files with the change.

When a ticket is done, its card gets the acceptance criteria, the manual test steps and the corner cases. It also names its tests in `qa_test`, as `area::module::test`, or `area::module` for several. You can add a corner case to the card later. Add a test here for it only if the case can break the binary, the API, MCP, the TUI or the render.

Property tests keep their failing seeds next to their module, as `qa/tests/api/fuzzing.proptest-regressions`; commit that file when proptest adds a seed so the case is replayed first on every run.

`scripts/legibility.py` is the YouTube-compression check: it renders a step, scales it to 1080p, round-trips it through H.264 with ffmpeg and measures the contrast every text color keeps. `export::video_legibility` and the Mocha step of `tui::terminal_theme` run it when python3 + Pillow + numpy + ffmpeg are installed (shared check `tiza_qa::legibility_tools_missing()`; otherwise they skip and print what is missing) and CI runs it in the `legibility` job (report and compressed frames as an artifact).

## Terminal size matrix with Herdr

A QA-round step on a machine with [Herdr](https://herdr.dev) (Luis's Mac), not part of `cargo test`: Herdr renders through libghostty-vt, a real terminal emulator, so it catches escape-sequence and redraw problems the pty snapshots cannot. From a Herdr pane at the repository root:

```sh
scripts/herdr-matrix.sh            # capture into qa/herdr/latest/ (git-ignored), print each case's size
scripts/herdr-matrix.sh --check    # capture and diff with qa/herdr/*.txt and *.ansi; exit 1 and list the panes that changed
scripts/herdr-matrix.sh --update   # capture and rewrite qa/herdr/*.txt and *.ansi on purpose (commit them with the change)
scripts/herdr-matrix.sh --normalize qa/herdr/vt100-80x24.ansi   # what --check compares of an ansi capture (no Herdr needed)
scripts/herdr-matrix.sh --check --file /tmp/copy.rs.tiza   # another document (default examples/ownership.rs.tiza, only read)
scripts/herdr-matrix.sh --base PANE_ID                     # split that pane instead of opening a tab
```

By default it works in a tab of its own: `herdr tab create --workspace W --cwd <repo> --label tiza-matrix`, where W is the current pane's workspace (`workspace_id` of `herdr pane layout --current`, or `HERDR_WORKSPACE_ID`), and it closes that tab with `herdr tab close` at the end, also on an error or Ctrl-C, so your panes are never split and there is always the whole window to work with. The tab id and its root pane come from the answer of `tab create` (`result.tab`, `result.root_pane`), or else from `herdr pane list` and the root pane's `pane layout`; if neither names them it stops and prints that JSON. `--base PANE_ID` splits one of your panes instead and closes every pane it opened, giving the space back.

The sizes are the **terminal's**, the ones Tiza sees: Herdr's `pane layout` rect (`rect.width`/`rect.height`) counts the pane's frame too (3 columns × 2 rows on Herdr 0.9.5: the b9f34fa version split 80×24 panes in which Tiza saw 77×22). So the script first measures the frame — it splits a probe pane, starts Tiza there and takes the rect minus what `tiza api status` reports — and prints it (`herdr frame: 3x2 cells …`); if Herdr changes its frame, the measure changes with it. For each case — `vt100` 80×24, `wide` 120×40, `zed-half` 75×50, `narrow` 60×30 and `full` (as large as the base pane allows, about 12 columns left to it), all `--present` at the last step of the example, plus `normal` 120×40 and `normal-narrow` 60×30 in normal mode with the status bar (the narrow one shows the small-terminal badge) — it splits one pane off the base already at target + frame, `herdr pane split BASE --direction right --ratio R` with R = 1 − (target_cols + frame)/base_width (the ratio is the share the base keeps), then splits that pane the same way with `--direction down` for a row target. A pane that misses by a cell or so is fixed with `herdr pane resize`, always in the direction that grows the pane resized: towards the base (`left`/`up`) on our pane to grow it, towards our pane (`right`/`down`) on its neighbour to shrink it — Herdr does not move an edge a pane does not share, so `resize right` on a pane at the right does nothing. Then it runs Tiza there with `--no-save` and a socket of its own; if Tiza does not see the target (a pane whose frame is not the measured one) the pane is resized by the difference, with a warning. It waits for two identical `herdr pane read --source visible --format text` (and, with each, an identical `--format ansi` read) and saves `qa/herdr/<case>-<cols>x<rows>.txt` named by the size **Tiza sees**, with the ansi capture next to it as `<case>-<cols>x<rows>.ansi`; the table prints the target, that size (`tiza`, the one that matters) and Herdr's rect (`herdr`). A case that misses its target, or a different window, shows up in `--check` as a size change rather than as a broken frame. Herdr 0.9.5 has no `pane wait-output`, so the cases run one after another and each one's panes are closed before the next. Without `herdr` on PATH, or with a Herdr that does not answer `herdr pane list`, it stops with a message before opening anything.

The `.txt` catches layout and redraw problems; the `.ansi` catches what it loses — wrong colours, bold/italic/underline, a stray SGR sequence. `--check` diffs the `.txt` byte for byte and the `.ansi` **by meaning, not by bytes**, since another Herdr version may write the same style differently: both captures are reduced to the text of each row (`T row|text`) and the runs of cells in one non-default style (`S row:first-last style`, columns counted in terminal cells from 1 — a wide character takes two —, style as `fg=`/`bg=`/`ul=` colours — `#rrggbb`, or `idxN` for the 16 palette colours the terminal's theme decides; 256-colour indexes 16–255 become their xterm RGB — plus attributes such as `bold`, `italic`, `underline` (or `underline-double`, `-curly`, `-dotted`, `-dashed`), `reverse`; a blank cell keeps only what shows on it, its background, unless it is reversed, underlined, struck or overlined). Every other escape sequence (cursor moves, OSC hyperlinks, DCS/APC strings, charset designations such as `\e(B`) and C0 control is dropped whole. So `\e[m` for `\e[0m`, `38:2::r:g:b` for `38;2;r;g;b`, one combined SGR for several, or CRLF line ends are no change, while an SGR code it does not know stays as `sgrN` and shows up. Each changed pane says which kind of change it is: `CHANGED <case>: text differs (…)` with the text diff, `CHANGED <case>: text differs in the ansi capture (N rows)` when the `.txt` matches but the `.ansi` shows other text, or `CHANGED <case>: style only (N rows …)` with the `-S`/`+S` runs that changed (the old style out, the new one in). A case whose snapshot has no `.ansi` yet (an old snapshot) asks for `--update`. `--normalize FILE` prints that reduced form of any ansi capture, to read one by hand.

Every `herdr` call goes through one function; with `TIZA_HERDR_FAKE=<state dir>` it is a model instead (tabs that `tab create`/`tab close` open and close, rects = terminal + frame — 3×2, or `TIZA_HERDR_FAKE_FRAME` — panes that are Python ptys of the terminal size running the real `tiza`, a pane that never shrinks by resizing it away from the edge it shares, and `pane read` is that instance's `tiza render` — `--format txt`, or `--format ansi` with CRLF line ends as Herdr prints them; `TIZA_HERDR_FAKE_SGR=other` writes the same styles in other SGR bytes), which is how `release::herdr_matrix_script` checks, without Herdr, the `--update`/`--check` flow with a changed label and the normalisation of the `.ansi` captures. The tab, `--base` and the cleanup on failure and on Ctrl-C are checked in Luis's QA round, with a real Herdr. The frames in `qa/herdr/` come only from a real Herdr run.

## MCP context trials

A QA-round step on Luis's Mac, not part of `cargo test` or the gate, because a real run calls a paid model: `scripts/agent-trials/run` launches headless Claude Code agents, one task and one settings variant each, connected to `tiza mcp` on a live Tiza of their own in a clean `/tmp` directory, and measures the context they use; success is checked by the harness through `tiza api`, never taken from the agent. `--dry-run` prints the plan, the commands and the estimated spend; `--self-check` runs every task's check against a scripted solution (it must fail before and pass after, and fail on a fixture broken with `--break`) with no model at all. A real run needs `--model`, prints the estimate and asks before it starts; start with `--max-trials 16` and only with Luis's OK. Method, tasks, variants, cost and results are in the maintainer notes, outside the repository.

## Size breakdown of the first baseline

Where the bytes of the release binary go, next to the baselines in `qa/budgets.json` and `qa/bench/baseline.json`. Measured on 2026-10-18 on Linux x86_64 at 5e57795 (fat LTO, the profile of the budgets; the committed Linux baseline at 5ab5bb1 is 13,158,152 bytes, this tree is 13,171,048 stripped, +0.1 %) with `cargo-bloat 0.12.1`: `CARGO_PROFILE_RELEASE_STRIP=false cargo bloat --release -p tiza --bin tiza --crates -n 0` for the code, `size -A` on the stripped binary for the sections, and a byte search in the stripped binary for each embedded blob. The Mac binary (11,209,360 bytes) was not broken down; Mach-O has smaller unwind and relocation tables, so expect the same proportions with a smaller "other sections" row. cargo-bloat attributes code by symbol name: generic code lands in the crate that instantiates it (most of ratatui and serde shows up under `tiza_*` and `serde_core`), so the per-crate code rows are approximate.

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

## Checklist for any card that adds or changes a `:` command

A typo must never close the app, leave the terminal broken or create something other than what was typed. Every rejection ends in a line that says how to go on (the expected shape, the valid values, or `:help`), and a success message appears only when the annotation is exactly what the user asked for. `tui::command_typos` drives this table through the real command runner; add the new command's own rows there.

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
| `:bx`, `:stpe`, `:colr` | `unknown command: did you mean :box?` (ties list up to three) |
| `:export x.png width=99999`, `tiza render --width 99999`, `--scale 99` | `width=99999: use a number between 20 and 400`, `scale=99: use a number between 0.25 and 8`; a PNG over 16384 px per side or 100 MP is refused naming the limit |
| any panic (`TIZA_TEST_PANIC=1` + `:panic` in a test) | the terminal is restored and one `tiza crashed: … Your work is in FILE.tiza … issues` line is printed |
| valid but odd input: emoji, RTL marks, tabs, `%s%n`, quotes, `$(id)`, a 20 000-character label | keeps working |
