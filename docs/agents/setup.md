# Connect an agent

Start the MCP server and add it to your MCP client.

With the TUI open, start the MCP server in another process:

```sh
tiza mcp
```

Configuration for Claude Code (the project's `.mcp.json` or `claude mcp add tiza -- tiza mcp`; if you had the server registered under the old name `codedraw`, remove it, add `tiza` and reconnect with `/mcp`):

```json
{ "mcpServers": { "tiza": { "command": "tiza", "args": ["mcp"] } } }
```

**After rebuilding Tiza** (`git pull && cargo build`, `cargo install`): an MCP client reads the tool list once per connection and keeps talking to the `tiza mcp` process it started, which is still the old binary. New or changed tools appear only after the client reconnects — in Claude Code: `/mcp` → `tiza` → *Reconnect* (or restart the session). The `status` tool says when this is needed: its `bridge` object carries the server's `version` and `build` (git commit, also in `tiza --version`) and sets `binary_rebuilt: true` with a hint once the binary on disk is newer than the running server, or `instance_build` when the TUI was built from another commit (restart the TUI too). `notifications/tools/list_changed` would not help here: the stale process cannot know the new one's tools.
