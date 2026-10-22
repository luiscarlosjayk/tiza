# ╭─ Tiza ─▶

![Tiza: step-by-step code annotations for the terminal. Three real frames of the Rust ownership example, steps 1, 4 and 7](https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/docs/img/hero.png)

**Draw on top of code in the terminal to explain it on video.**

Tiza is a Rust TUI (ratatui) built for tutorials: you send the code to Tiza in a terminal and draw on top of it (boxes, arrows, highlights, notes and semantic marks such as *warn*, *read*, *write*, *move* or *drop*) to explain a concept step by step. Annotations are anchored to the code text, not to screen cells, so they survive scrolling, resizing and theme changes. Every step exports to text, Markdown, ANSI, SVG, PNG, GIF, MP4 (with your `ffmpeg`), asciicast or VHS.

It is also **for agents**: a local socket API and an MCP server (`tiza mcp`) let Claude Code, or any MCP client, draw live on what you are showing.

## Installation

```sh
cargo install tiza
```

Rust ≥ 1.88; macOS and Linux (Windows is not supported in v0.x). Prebuilt binaries for macOS (Apple Silicon and Intel) and Linux are in [Releases](https://github.com/luiscarlosjayk/tiza/releases).

## Quick start

```sh
tiza src/main.rs            # open a file
tiza src/main.rs:12-48      # only a range of lines (numbering is preserved)
tiza src/main.rs --follow   # reload when the file is saved in the editor
pbpaste | tiza              # code from the clipboard (macOS); --lang rs when it cannot be guessed
```

Keys, `:` commands, exports, the MCP tools and the `.tiza` format are documented in the [full README](https://github.com/luiscarlosjayk/tiza#readme) and the [agent guide](https://github.com/luiscarlosjayk/tiza/blob/main/docs/agent-guide.md); the demos are in [examples/](https://github.com/luiscarlosjayk/tiza/tree/main/examples).

The other `tiza-*` crates are internal: published only so that `cargo install tiza` works, with no stable API.

## License

MIT or Apache-2.0, at your option.
