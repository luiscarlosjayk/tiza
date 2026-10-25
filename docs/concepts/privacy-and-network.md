# Privacy and network

This page tells what stays on your computer when you use Tiza, and why. [Files and environment](../reference/files-and-environment.md) lists each file, socket, process and environment variable.

## The network

The `tiza` binary has no network dependencies. It never opens a network connection.

- Agents connect through a local Unix socket, never through TCP.
- The MCP server, `tiza mcp`, talks to the agent on its standard input and output.
- Tiza has no telemetry, no crash reports and no update check. It has no identifier of the user or of the machine.

A future network feature goes in a separate binary, `tiza-net`. It runs only when you give it a command. Before it sends data, it tells you what it sends. Decided in [ADR 0009](../adr/0009-politica-de-red.md).

## What Tiza reads and writes

All of this stays on your computer:

- Tiza reads the files that you open, the files that a document names, its config file and a theme file that you give.
- Tiza writes the `.tiza` document, the copies of the document and the exports that you ask for.
- By default, Tiza keeps its socket and its log, `panics.log`, in a runtime folder. Tiza tries to set that folder to `0700`, so that only your user can open it.
- Tiza starts `ffmpeg` for an MP4 or a WebM export. When the terminal does not give its size, the terminal library can run `tput`.

The full list is in [Files and environment](../reference/files-and-environment.md).

## Paths in a document

A document names its code and its linked files with a path. A path can be relative or absolute.

> **Warning:** Tiza can read any file that you can read. Thus a document from another person can show a file of yours on the screen, or put it in an export. Read the paths in a document from another person before you open it.

## Agents

An agent sends documents, code and commands through the socket, or through `tiza mcp`. The agent can get the code from Tiza.

- When an agent sends a document, Tiza reads the files that the document links.
- An agent can export an image to a path.

Connect only the agents that you trust with the code on your screen.

## How it works

- [`deny.toml`](../../deny.toml) bans a list of network crates for the whole workspace, for example `reqwest`, `hyper` and `rustls`. It also bans the `net` feature of `tokio`.
- CI runs `cargo-deny` on each push to `main` and on each pull request. Thus a change that adds one of these crates makes CI fail.
- `cargo-deny` does not see code that uses `std::net` directly.
- To check this yourself, read `Cargo.lock`. It has no HTTP or TLS crate.
