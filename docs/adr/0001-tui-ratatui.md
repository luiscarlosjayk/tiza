# 0001 · CodeDraw is a TUI built with ratatui

**Status:** accepted (2026-10-15)

## Context

The use case is explaining code on video while sharing the screen with the editor (Zed). The tool has to live alongside the editor in a terminal (Zed's integrated one, Ghostty, WezTerm), start instantly and accept code from stdin or the clipboard. A native desktop app with `gpui` was considered.

## Decision

A Rust TUI with **ratatui 0.30** + **crossterm 0.29**. The core (`codedraw-core`) does not depend on ratatui: it produces its own cell canvas, which the TUI copies into its buffer and the CLI dumps as text/ANSI. Keyboard first, with a `:` command line (vim style); the mouse will be opt-in.

## Consequences

- We inherit the user's terminal font and size: legibility on video depends on their configuration, not ours. We document a recording profile.
- Real width at 1080p with a large font: ~75–100 columns. The layout must degrade gracefully (ADR 0002).
- No freehand drawing: the visual language is that of box-drawing characters and Unicode arrows.
- Declared inspiration: Excalidraw (concept), excalidraw/cli (CLI for humans and agents), herdr (socket API), tuios (vim modal + mouse).
