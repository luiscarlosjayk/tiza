# For agents working in this repository

This file sends you to the page that your task needs. Read only the pages of your task.

Some pages are maintainer notes outside this repository, in `../dev/` next to the checkout. If you do not have that folder, follow the rules on this page.

## Always

- Run this before every push: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test`. Every command must pass.
- Do not push a change that adds a CI failure. If CI fails on your commit, fix it first.
- Write every user-facing string in English. Commands, JSON fields and the Spanish color aliases stay as they are.
- Write public text in ASD-STE100 Simplified Technical English. The targets are in `../dev/writing-docs.md`.
- No ticket ids (`t-` and three digits) anywhere in the repository: not in code, comments, docs, schemas, the CHANGELOG, user-facing strings, the names of tests and fixtures, or commit messages. A QA test fails on an id in a file, with its file and line.
- Do not put private data in the repository. Do not write a path of a home folder. Do not link to a session or to a private page.
- Write commit messages in English, in the imperative mood. Keep the subject to 72 characters or fewer. Put the detail in the body.
- Do not add a `Co-Authored-By: Claude…` or a `Claude-Session:` trailer to a commit.
- Do not create or delete a tag. The owner tags a release.

## Find the page for your task

| Task | Read |
|---|---|
| Learn the crates and build Tiza | [CONTRIBUTING.md](CONTRIBUTING.md#development) |
| Add or change a test | [qa/README.md](qa/README.md) |
| Change a TUI surface, a panel or a message | [crates/tiza-tui/UI-GUIDELINES.md](crates/tiza-tui/UI-GUIDELINES.md) |
| Change the socket protocol or an MCP tool | [socket API](docs/reference/socket-api.md), [MCP tools](docs/reference/mcp-tools.md), [agent guide](docs/agent-guide.md) |
| Change the `.tiza` document format | [document format](docs/reference/document-format.md) |
| Add a dependency | `../dev/adr/0004-dependencias.md` (criteria and registry) |
| Write or change a page of the docs | `../dev/writing-docs.md`, `../dev/doc-roles.md` |
| Change what the renderer draws | [Tests, images and budgets](#tests-images-and-budgets) |
| Plan a card or write a design | `../dev/lessons/README.md` (the checklist, then the file of your area) |
| Make a design decision | `../dev/adr/` (one new record for each decision) |
| Publish a release | [CONTRIBUTING.md](CONTRIBUTING.md#releases-maintainers) |

## Design rules

- `tiza-core` has no terminal dependency. Only `tiza-export` links resvg. Only the MCP bridge uses tokio: `tiza-mcp`, and `tiza mcp` in the binary.
- Tiza stores the position of an annotation as a line and a grapheme offset. It converts the position to cells only when it draws.
- Change the document only through the local socket. Send one JSON request per line. Do not use TCP.
- Each MCP tool is one socket method. When you change the protocol, change the docs, the MCP tools and `schema/api.schema.json` in the same commit.
- When you change the `.tiza` schema, add a migration in `fs::MIGRATIONS` and a file in `qa/fixtures/compat/`.
- When you change the API or the document format, regenerate `schema/api.schema.json` (`tiza schema api`) and `schema/tiza.schema.json` (`tiza schema document`), and run `scripts/check-schema.py`.
- A move re-anchors annotations by its own edit, never by a fuzzy text match. The document at step N is the same each time.
- Make exports reproducible. Do not use system fonts unless a command asks for them (`--system-fonts`).
- Check a new dependency against ADR 0004. Add a row for it to the registry in that ADR.
- The `tiza` binary, `tiza-core`, `tiza-tui`, `tiza-export` and `tiza-mcp` never open a network connection. `deny.toml` bans the HTTP and TLS crates.
- A network feature goes only in a separate crate, `tiza-net`. Write an amendment to ADR 0009 before you add it.

## Tests, images and budgets

- Add an automated test only for what can break. Test the behavior of the binary, the API, MCP, the TUI or the exports.
- Add a test for the regression of each bug that you fix.
- Put a test in the crate of its area in `qa/tests/<area>/` (render, tui, follow, api, steps, export, saving, check, mcp, release, policy), in the module of the behavior that it checks. Give it a name that tells the behavior.
- Put its fixtures in `qa/fixtures/<area>/<module>/`, or in `qa/fixtures/shared/` when tests of several areas read them. A test that changes the environment of its own process goes in `env`. A test that checks how fast a child process answers goes in `latency`. A test that checks a cache of its whole process goes in the crate `cache`. A test that runs a TUI with follow on a pty goes in the crate `watchers`.
- A test must catch a bug or a broken user experience. Do not write a test that only checks a sentence or a heading.
- The tests of qa/ test the code (the binary, the API, MCP, the TUI, the exports) and the policies of the repository (ticket ids, licenses). They do not check what the docs say. The checks of the docs run only in the job that builds the site.
- Put a check of the docs against the code in `site/checks/`. When you change the docs or the code that they describe, run `cargo test -p tiza-site-checks`, or check the "Docs site" job of your pull request. Before a release, `scripts/release-check.py` checks the docs that the release reads.
- Do not use `#[ignore]`. Do not retry a test blindly. Do not use a sleep to synchronize. Wait on a condition with a timeout of 10 s.
- To test the TUI without an interactive terminal, use a pseudo-terminal (Python `pty`) and `tiza api status --socket PATH`.
- `examples/` holds demos only, never test data.
- When you change what the renderer draws, run `scripts/readme-images.sh` in the same commit.
- When you change a panel, the status bar or presentation mode, run `scripts/tui-captures.sh` in the same commit.
- A frame in a doc or a brand image is a real `tiza render` export. Do not edit it. Do not make a mockup.
- An image of the TUI is the only other kind of image of Tiza in a doc. Use it only for what `tiza render` does not draw: presentation mode, the status bar, the live pointer, the focus, the padding and the layout of normal mode, a panel or the view of a `scroll` move. Make it with `scripts/tui-captures.sh`. Do not edit it.
- Keep images out of the repository unless they are necessary. Tests use text exports, not PNG files.
- Before you push a new dependency or an embedded asset, run `scripts/size-check.sh` and `scripts/bench.sh`. CI does not run them.
- The limits are in `qa/budgets.json` and `qa/bench/baseline.json`.
