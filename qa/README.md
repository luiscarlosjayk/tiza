# QA tests, one file per board task

`qa/tests/t-XXX.rs` is the automated side of the card **t-XXX** on the board: its header repeats the task's acceptance criteria and each `#[test]` is one of the card's test steps or corner cases, named `acN_…`. Cargo compiles every file here as an independent crate, so one ticket's test can be run alone:

```sh
cargo test -p tiza-qa                 # every ticket
cargo test -p tiza-qa --test t-092    # one ticket
cargo test -p tiza-qa --test t-050 -- ac3 --nocapture
```

Test data lives in `qa/fixtures/t-XXX/` (never in `examples/`, which are the demos people open). `tiza_qa` (in `src/lib.rs`) gives the helpers: `bin()` builds and locates the `tiza` binary, `tiza_ok(&[…])` runs it, `fixture("t-092/callouts.tiza")`, `scratch(name)` for output, and `Live` — a headless instance with the real `App` and the real socket server, so a test can call the API (`live.api_ok("annotation.add", json!({…}))`), press keys (`live.keys(&[KeyCode::Char('V')])`), run `:` commands and read the screen (`live.screen(100, 30)`).

`McpClient` (in `src/lib.rs`) is a real JSON-RPC client over stdio for `tiza mcp`: handshake, `tools/list`, one `tools/call` at a time with the reply read in order, so reconnection and errors can be observed (t-070).

Render snapshots (t-053) live in `qa/tests/snapshots/` (insta): one `-symbols` and one `-styles` file per case. When a render changes on purpose, run `INSTA_UPDATE=always cargo test -p tiza-qa --test t-053` (or `cargo insta review`) and commit the updated `.snap` files with the change.

When a ticket is done, its card gets: acceptance criteria, manual test steps, corner cases and the path of this file (`qa_test`). Keep the three in sync: if a corner case is added to the card, add the test here.

Property tests (`t-074`) keep their failing seeds in `qa/tests/t-074.proptest-regressions`; commit that file when proptest adds a seed so the case is replayed first on every run.

`scripts/legibility.py` (t-072) is the YouTube-compression check: it renders a step, scales it to 1080p, round-trips it through H.264 with ffmpeg and measures the contrast every text color keeps. `qa/tests/t-072.rs` runs it when python3 + Pillow + numpy + ffmpeg are installed and CI runs it in the `legibility` job (report and compressed frames as an artifact).

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
