# ADR 0009: Network policy — the `tiza` binary never opens a connection

- Status: accepted (2026-10-18, Luis)
- Board: t-118; read by whichever card brings the first HTTP feature (t-117, redaction, read it too; that feature was withdrawn by t-183, ADR 0010)
- Related: [ADR 0003](0003-api-socket-y-mcp.md) (the local API is a Unix socket, never TCP), [ADR 0004](0004-dependencias.md) (dependency policy, `cargo-deny`), [docs/research/demo-time.md](../research/demo-time.md) (the remote-control PWA was discarded on 2026-10-17)

## Context

Tiza shows code on screen — sometimes code with secrets in it (t-117 added masks for that; t-183 withdrew them). A presenter has to be able to trust that what Tiza shows stays on the machine. At the same time, some day we will add something that talks to the internet: publishing a GIF or a `.cast` to a service, a new-version notice, downloading themes or fonts. If the promise in the README is written as «Tiza never uses the network», that day either breaks the promise or ships the feature as an exception nobody planned. The policy is written now, before any such feature exists, so the promise survives it and the first HTTP feature arrives with its structure already decided.

Today the workspace has no network dependency at all: `rmcp` is built with `transport-io` only (stdio), `tokio` without the `net` feature, `mio` is there for terminal and file events (crossterm, notify, signal-hook), and `resvg` / the font loader read only embedded or local files. `Cargo.lock` has no `reqwest`, `hyper`, `ureq`, `curl`, `native-tls` or `rustls`.

## Decision

### The promise (final wording, reused verbatim by the README)

> **The `tiza` binary that shows your code has no network dependencies and never opens a connection.** Its only I/O is your terminal, the files you name, the `.tiza` sidecar next to them, and a local Unix socket for agents (never TCP). Features that talk to the internet live in a separate crate and binary, are off unless you run them, and state what they send before they send it.

### The structure

1. The core crates — `tiza-core`, `tiza-tui`, `tiza-export`, `tiza-mcp` and the `tiza` binary (`crates/tiza-cli`) as built by default — are **network-free**. `deny.toml` enforces it: the network crates are banned for the whole workspace today, and the ban stays on these crates for good.
2. Every network dependency goes into **one crate, `tiza-net`**, which does not exist yet and is created by the first feature that needs it. It ships as its own binary (`tiza-net`, or subcommands of a separate binary — the amendment that creates it decides the name) so that installing `tiza` never pulls it in. It is **not** a default workspace member of the release build: the release workflow builds and packages it only when an ADR amendment says so, as a separate artifact.
3. A cargo feature (`net`) on the main binary is **not** the default structure: a feature can be flipped on by a packager and the user would not know. If one day the two binaries are a real burden, an amendment may merge them — with the feature off by default and the `--version` output stating whether it is built in.
4. `deny.toml` carries the list of banned crates with a comment pointing here. When `tiza-net` is created, the bans are scoped with `wrappers`/`skip` to exempt only that crate, never the core.

### The rules for any network feature

- **Explicit action per use.** A command or a flag the user runs (`tiza-net publish demo.gif`, `tiza-net update-check`). Never on start-up, never on a timer, never because a file was opened, never as a side effect of another command.
- **Disclosure before sending.** One line, before the request leaves: what is sent, where to, and (when there is one) what account it is sent as. `--yes` skips the prompt, not the line.
- **Code and annotations never leave the machine unless that is the feature's purpose and the user asked for it** — publishing a `.tiza` is such a purpose; checking for a new version is not and sends nothing about the document.
- **What is sent is what is rendered.** An upload carries what a render of the document shows, nothing else. (This rule said «masks first» while the redact feature of t-117 existed; t-183 withdrew it, ADR 0010.)
- **No identifiers, no telemetry.** No machine id, no install id, no usage counters, no crash reports. A request carries what the feature needs and the client's `User-Agent` (`tiza-net/<version>`), nothing else. There is no «anonymous statistics» opt-out because there are no statistics.
- **No credentials in the core.** Tokens for a publishing service live in the `tiza-net` configuration (its own file or the keychain), never in a `.tiza` and never in the core's config.
- **Offline is the normal state.** A missing network is not an error the core ever sees; `tiza-net` says «could not reach X» and does nothing else.

### Foreseeable cases, first-pass decisions

| Case | Decision now |
|---|---|
| Publish a GIF / `.cast` / `.png` / `.svg` to a service (asciinema.org, a Gist, an S3 bucket) | Yes, as `tiza-net publish FILE` when a card asks for it: the file is produced locally first by `tiza render` (so the user can look at it), the upload is a second explicit step with the disclosure line, and the `.tiza` itself is uploaded only with an explicit flag. The service, endpoint and payload go in the amendment. |
| New-version notice | Only `tiza-net update-check`, run by the user; it sends the request to the release endpoint with no payload beyond the version it is comparing against (in the User-Agent). Never automatic, never from `tiza` itself, no «check on start-up» setting. |
| Downloading themes or fonts | Fonts ship inside the binary (ADR 0006) and themes are text (t-119): a user can paste a theme file; `tiza-net theme get NAME` may fetch one from the project's own repository when a card asks, with the URL shown first. Nothing is fetched at render time. |
| HTTP remote control / clicker / PWA | Discarded on 2026-10-17 with the Demo Time decisions (product criterion: no speaker tooling). Stays out unless Luis reopens it; if he does, it is a `tiza-net` feature bound to localhost by default, with the ADR amendment saying so. |
| Telemetry / crash reporting | No. A crash prints one line with how to report it by hand (t-115). |

### How a new case is added

An amendment to this ADR, in the same commit as the feature, naming: the trigger (command or flag), the endpoint(s), the payload (fields, and that code/annotations are or are not in it), the disclosure line as printed, the off-switch (how to make sure it never runs — not installing `tiza-net` is always enough), and the `deny.toml` change. The README section «What Tiza reads and writes» gets the one line that describes the feature. A card that adds a network dependency to a core crate is sent back to rework.

## Consequences

- Today: no code changes beyond `deny.toml` (the bans), the README section and a line in `AGENTS.md`. `cargo deny check bans` fails the moment a core crate pulls `reqwest`, `hyper`, `ureq`, `curl`, `native-tls`, `rustls`, `openssl` or a `tokio` with the `net` feature.
- The first HTTP feature costs one more crate and one more artifact in the release workflow; that is the price of a promise that can be verified by reading `Cargo.lock`.
- `tiza-mcp` keeps talking to agents over stdio and the Unix socket only; an MCP transport over HTTP (streamable HTTP in the MCP spec) would be a `tiza-net` feature with its own amendment, not a change to `tiza-mcp`.
- The research note row «Remote control PWA / clicker» stays as recorded (out of scope); this ADR only adds where it would live if that decision were reopened.
