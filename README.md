![Tiza: step-by-step code annotations for the terminal. Three real frames of the Rust ownership example, steps 1, 4 and 7](docs/img/hero.png)

**Draw on top of code in your terminal to explain, debug and teach.**

Tiza is an interactive terminal tool for visual annotations on code. Use it to give a walkthrough, to present on screen, to record a video or to work through a bug with an LLM. AI agents can draw on the same code through MCP.

Send code to Tiza from your editor. Then draw on it, highlight what is important and go through it step by step.

The annotations are boxes, arrows, highlights, notes and semantic marks such as *warn*, *read*, *write*, *move* or *drop*, plus any style you define, emoji included. Annotations are anchored to the code text, not to screen cells, so they survive scrolling, resizing and theme changes.

It is also **for agents**: it exposes a local socket API and an **MCP server**, so Claude Code (or any MCP client) can draw live on what you are showing.

![Tiza explaining Rust ownership step by step](docs/img/ownership.gif)

> Status: **v0.1.0**, early: the document format and the API may still change before 1.0. Inspired by [Excalidraw](https://excalidraw.com), [excalidraw/cli](https://github.com/excalidraw/cli), [herdr](https://herdr.dev), [tuios](https://github.com/Gaurav-Gosain/tuios) and [Demo Time](https://demotime.show).

<!-- readme-images:ownership-txt -->
```
  1          fn main() {
  2 ╭───────     let s = String::from("hello");
    │                ◀───╯                                             s is born
  3 │            takes(s);                                             ━▶ move: into takes
  4 │            // println!("{s}"); // error: borrow of moved value   ┆ s no longer owns it: using it here
  5 │                                                                    does not compile
  6 │            let t = String::from("world");
  7 │ ╭╌╌╌╌╌     let n = len(&t);                                      ╌▶ read: borrowed
  8 │ ╎          println!("{t} has {n} letters");                      ┆ t is still valid
  9 │ ╎
 10 │ ╎          let mut u = String::from("hey");
 11 │ ╎ ┏━━━     push(&mut u);                                         ✎▶ &mut borrow
 12 │ ╎ ┃        println!("{u}");                                      ┆ u is now "hey!"
 13 │ ╎ ┃    }
 14 │ ╎ ┃
    │ ╎ ┃ ╭─ takes owns s and frees it at the end ───────────────────╮
 15 ╰─╎─┃─┼▶ fn takes(s: String) {                                   │ move
 16   ╎ ┃ │      println!("{s}");                                    │
 17   ╎ ┃ │  }                                                       │ ╳ drop: s dies here
      ╎ ┃ ╰──────────────────────────────────────────────────────────╯
```
<!-- /readme-images:ownership-txt -->

Every image in this README is made by Tiza itself, with no screen recording: `scripts/readme-images.sh` renders the GIF at the top (the native `.gif` export), this step-7 PNG, the text block under the GIF and the three real `--step` frames of the hero image (`docs/img/src/hero.html`, screenshotted by a headless Chrome or Chromium — a docs-only tool, skipped with a message when none is installed), all from `examples/ownership.rs.tiza` at `--width 108`. Running it twice gives the same bytes.

## Installation

Tiza is written in Rust with [ratatui](https://ratatui.rs).

On macOS and Linux, run this command to install the latest release:

```sh
curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh
```

The script downloads the archive for your system from [Releases](https://github.com/luiscarlosjayk/tiza/releases) and checks its SHA-256 checksum. Then it puts `tiza` in `~/.local/bin`. It does not use `sudo`. If that folder is not in your `PATH`, the script shows the line to add.

**From source:** with Rust 1.88 or later, install from crates.io or from `main` of the repository:

```sh
cargo install tiza
cargo install --git https://github.com/luiscarlosjayk/tiza tiza
```

## Quick start

Try the bundled demos (see [examples/README.md](examples/README.md)): `tiza examples/ownership.rs.tiza` starts with the bare code; advance with `n`.

## Documentation

- Install: [other ways to install and update](docs/guides/install.md)
- Use: [command line](docs/reference/cli.md), [keys](docs/reference/keys.md), [`:` commands](docs/reference/commands.md), [document format](docs/reference/document-format.md), [saving](docs/concepts/saving.md)
- Present: [follow the editor](docs/guides/follow-the-editor.md), [mouse](docs/guides/present.md), [check before you record](docs/guides/check-before-you-record.md), [exports](docs/reference/exports.md)
- Terminals and colors: [terminal support](docs/reference/terminals.md), [themes](docs/reference/themes.md)
- Agents: [connect an agent](docs/agents/setup.md), [MCP tools](docs/reference/mcp-tools.md), [supervised mode](docs/agents/supervised-mode.md), [socket API](docs/reference/socket-api.md), [agent guide](docs/agent-guide.md)
- Privacy: [what Tiza reads and writes](docs/concepts/privacy-and-network.md)
- Contribute: [development](docs/contributing/development.md), [releases](docs/contributing/releasing.md)

## Zed integration

A task that sends the current selection to Tiza in the integrated terminal: [docs/zed.md](docs/zed.md).

## Feedback and bugs

Report a bug that you can reproduce with the [bug report form](https://github.com/luiscarlosjayk/tiza/issues/new/choose). Put ideas, feature requests, feedback and questions in [Discussions](https://github.com/luiscarlosjayk/tiza/discussions). Tiza does not accept pull requests for now: [CONTRIBUTING.md](CONTRIBUTING.md) tells why and how to help. Report a vulnerability in private, as [SECURITY.md](SECURITY.md) tells.

## License

MIT or Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Tiza by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

Made by [Luis Carlos Osorio Jayk](https://github.com/luiscarlosjayk) (LuisK) · [LinkedIn](https://www.linkedin.com/in/luiscarlososoriojayk) · [YouTube](https://www.youtube.com/@luiskdev).
