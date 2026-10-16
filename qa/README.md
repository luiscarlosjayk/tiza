# QA tests, one file per board task

`qa/tests/t-XXX.rs` is the automated side of the card **t-XXX** on the board: its header repeats the task's acceptance criteria and each `#[test]` is one of the card's test steps or corner cases, named `acN_…`. Cargo compiles every file here as an independent crate, so one ticket's test can be run alone:

```sh
cargo test -p codedraw-qa                 # every ticket
cargo test -p codedraw-qa --test t-092    # one ticket
cargo test -p codedraw-qa --test t-050 -- ac3 --nocapture
```

Test data lives in `qa/fixtures/t-XXX/` (never in `examples/`, which are the demos people open). `codedraw_qa` (in `src/lib.rs`) gives the helpers: `bin()` builds and locates the `codedraw` binary, `codedraw_ok(&[…])` runs it, `fixture("t-092/callouts.codedraw")`, `scratch(name)` for output, and `Live` — a headless instance with the real `App` and the real socket server, so a test can call the API (`live.api_ok("annotation.add", json!({…}))`), press keys (`live.keys(&[KeyCode::Char('V')])`), run `:` commands and read the screen (`live.screen(100, 30)`).

`McpClient` (in `src/lib.rs`) is a real JSON-RPC client over stdio for `codedraw mcp`: handshake, `tools/list`, one `tools/call` at a time with the reply read in order, so reconnection and errors can be observed (t-070).

Render snapshots (t-053) live in `qa/tests/snapshots/` (insta): one `-symbols` and one `-styles` file per case. When a render changes on purpose, run `INSTA_UPDATE=always cargo test -p codedraw-qa --test t-053` (or `cargo insta review`) and commit the updated `.snap` files with the change.

When a ticket is done, its card gets: acceptance criteria, manual test steps, corner cases and the path of this file (`qa_test`). Keep the three in sync: if a corner case is added to the card, add the test here.

Property tests (`t-074`) keep their failing seeds in `qa/tests/t-074.proptest-regressions`; commit that file when proptest adds a seed so the case is replayed first on every run.
