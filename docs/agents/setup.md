# Connect an agent

Add the Tiza MCP server to your MCP client. The client starts `tiza mcp`.

Your agent can do the setup. Give it this prompt:

```text prompt
Set up the Tiza MCP server for me. Follow https://tiza.luisk.dev/agents/setup/. If that page does not load, follow https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/docs/agents/setup.md.
```

To do the setup yourself, use the commands for your client below.

## Steps for an agent

Do these steps when the person asks you to set up Tiza.

1. Run `tiza --version`. When the shell does not find `tiza`, install it with the command in [Install Tiza](../guides/install.md). On macOS and on Linux, this is the command:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh
   ```

   The script tells you when `~/.local/bin` is not in the `PATH`. Then tell the person the line to add.
2. Add the server for your client. The commands are in the next sections.
3. Ask the person to connect the client again. In Claude Code, they type `/mcp`, select `tiza` and select *Reconnect*. In Codex, they restart the session.
4. Call the `status` tool. When no Tiza is open, the call fails with `No Tiza instance is listening on PATH`. This error shows that the server works. Ask the person to run `tiza FILE` in a terminal, then call `status` again.

## Claude Code

Add the server one time:

```sh
claude mcp add tiza -- tiza mcp
```

You can also put this configuration in the `.mcp.json` file of the project:

```json
{ "mcpServers": { "tiza": { "command": "tiza", "args": ["mcp"] } } }
```

If you added the server under the old name `codedraw`, remove it. Then add `tiza` and reconnect with `/mcp`.

## Codex

Add the server one time:

```sh
codex mcp add tiza -- tiza mcp
```

You can also put this configuration in `~/.codex/config.toml`. For one project, put it in `.codex/config.toml` of that project. Codex reads the project file only in a trusted project.

```toml
[mcp_servers.tiza]
command = "tiza"
args = ["mcp"]
```

Type `codex mcp list` to see the server. In the Codex TUI, type `/mcp`.

## Other MCP clients

Use this configuration:

```json
{ "mcpServers": { "tiza": { "command": "tiza", "args": ["mcp"] } } }
```

## Start the server yourself

With the TUI open, you can start the MCP server in another process:

```sh
tiza mcp
```

## After you rebuild Tiza

You rebuild Tiza with `git pull && cargo build` or with `cargo install`. An MCP client reads the tool list one time for each connection. It keeps the `tiza mcp` process that it started, and that process is the old binary. New or changed tools show only after the client connects again.

- In Claude Code, type `/mcp`, select `tiza`, then select *Reconnect*. You can also restart the session.
- In Codex, restart the session.

The `status` tool tells you when to connect again:

- Its `bridge` object has the `version` and the `build` of the server. The build is the git commit, as in `tiza --version`.
- It sets `binary_rebuilt: true` and gives a hint when the binary on disk is newer than the server that runs.
- It sets `instance_build` when the TUI comes from another commit. Then restart the TUI too.

## How it works

The `notifications/tools/list_changed` message does not help here. The old process cannot know the tools of the new binary.
