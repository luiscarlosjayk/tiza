# Security policy

## Supported versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |
| < 0.1   | No        |

## Report a vulnerability

Do not report a vulnerability in a public issue, a discussion or a pull request.

Use the private vulnerability reporting of GitHub:

1. Open the **Security** tab of this repository.
2. Click **Report a vulnerability**. The direct link is <https://github.com/luiscarlosjayk/tiza/security/advisories/new>.
3. Fill in the form.

Only you and the maintainer can read the report.

## What to include

- The version: the output of `tiza --version`.
- The operating system and its version.
- The steps that show the problem, as short as possible. Attach the smallest `.tiza` file or source file that shows it.
- The effect: what an attacker can read, write or run, and from where.

## Scope

These parts of Tiza are in scope:

- **The local socket and its session.** Each instance listens on a Unix socket in `tiza/` inside `$XDG_RUNTIME_DIR`, `$TMPDIR` or `/tmp`, or at the path that `--socket PATH` gives, with 0600 permissions. It never listens on TCP.
- **The MCP server.** `tiza mcp` lets an agent draw on the code that you show. Tiza saves these changes in the `.tiza` file.
- **The files that Tiza writes.** These are the `.tiza` sidecar, the files of `:w PATH` and `:saveas PATH`, and the exports.
- **The network policy.** The `tiza` binary never opens a network connection. Only a separate `tiza-net` crate can use the network ([ADR 0009](docs/adr/0009-politica-de-red.md)).
- **Untrusted content.** This is a `.tiza` file from another person, the files that it references, and code that you paste into Tiza.

## Response

Tiza has one maintainer. Replies are best effort, and there is no fixed response time. Tiza has no bug bounty.
