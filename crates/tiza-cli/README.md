# ╭─ Tiza ─▶

![Tiza: draw on top of code in your terminal to explain, debug and teach. Three real Tiza frames of the ownership example: steps 1, 4 and 7](https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/docs/img/hero.png)

**Draw on top of code in your terminal to explain, debug and teach.**

Tiza is an interactive terminal tool for visual annotations on code. Use it to give a walkthrough, to present on screen, to record a video or to work through a bug with an LLM. AI agents can draw on the same code through MCP.

Send code to Tiza from your editor. Then draw on it, highlight what is important and go through it step by step.

The annotations are boxes, arrows, highlights, notes and semantic marks such as *warn*, *read*, *write*, *move* or *drop*. Annotations are anchored to the code text, not to screen cells, so they survive scrolling, resizing and theme changes. Every step exports to text, Markdown, ANSI, SVG, PNG, GIF, MP4 (with your `ffmpeg`), asciicast or VHS.

It is also **for agents**: a local socket API and an MCP server (`tiza mcp`) let Claude Code, or any MCP client, draw live on what you are showing.

## Installation

Tiza is written in Rust with [ratatui](https://ratatui.rs).

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

Read the [keys](https://github.com/luiscarlosjayk/tiza/blob/main/docs/reference/keys.md), the [`:` commands](https://github.com/luiscarlosjayk/tiza/blob/main/docs/reference/commands.md), the [exports](https://github.com/luiscarlosjayk/tiza/blob/main/docs/reference/exports.md), the [MCP tools](https://github.com/luiscarlosjayk/tiza/blob/main/docs/reference/mcp-tools.md) and the [`.tiza` format](https://github.com/luiscarlosjayk/tiza/blob/main/docs/reference/document-format.md). The [README](https://github.com/luiscarlosjayk/tiza#readme) links to the user pages. The [agent guide](https://github.com/luiscarlosjayk/tiza/blob/main/docs/agent-guide.md) is for AI agents. The demos are in [examples/](https://github.com/luiscarlosjayk/tiza/tree/main/examples).

The other `tiza-*` crates are internal: published only so that `cargo install tiza` works, with no stable API.

## License

MIT or Apache-2.0, at your option.
