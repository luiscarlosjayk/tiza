# MCP context trials (t-161)

How much context does an agent spend to use Tiza through `tiza mcp`, and what changes it? This page holds the method, the harness that measures it and the results. It is the first card of a chain: measure before changing anything (t-161), then re-run the same trials after each change — instructions and always-load (t-162), lean schemas (t-163) — and decide (t-160).

## What the server offers today

Measured with a JSON-RPC `initialize` + `tools/list` against `tiza mcp` at 3970798 (no model involved):

| | 7b815cf (the card) | 3970798 |
|---|---|---|
| tools | 27 | 31 |
| `tools/list` (compact JSON) | 29,916 bytes | 34,178 bytes |
| input schemas | ~24 KB | 24,223 bytes |
| descriptions | 5,490 chars | 8,185 chars |
| server instructions | 766 chars | 917 chars |

At roughly four bytes per token, the whole tool list is about 8,500 tokens when it is loaded upfront. In a Claude Code session the Tiza tools are deferred by default (tool search), so that cost is paid only for the tools an agent looks up — which is what the trials measure.

## The harness

`scripts/agent-trials/run` (Python 3, standard library only). It is a QA-round tool, never part of `cargo test` or the gate: a real run calls a paid model.

Each trial is one fresh headless agent with **one task** and **one settings variant**:

1. A new directory under `/tmp` holds only the task's fixture (`scripts/agent-trials/fixtures/`), so the agent can read neither the repository nor the docs: the trial measures the MCP, not the docs.
2. A live Tiza opens the fixture on a pseudo-terminal of 120×40, with `--no-save` and a socket of its own.
3. `claude -p` runs in that directory with `--output-format stream-json --verbose`, the model given by `--model`, and:
   - `--strict-mcp-config --mcp-config` naming only this trial's `tiza mcp --socket …`;
   - `--tools ""` (no built-in tools: the agent cannot read files or run commands; the harness stops the run if the tool-search variant then lacks `ToolSearch`, see `--builtin-tools`);
   - `--allowedTools 'mcp__tiza__*,ToolSearch' --permission-mode dontAsk`, so the Tiza tools and tool search run and anything else is denied;
   - `--setting-sources project` (the trial directory has no project settings, so none of the machine's user settings — hooks, plugins, output style — apply), `--disable-slash-commands --no-session-persistence`, and `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`, `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` (no CLAUDE.md, no memory); the session's plugins, skills and permission mode from its init message are kept with each trial, so a run on another machine can be compared;
   - `--max-budget-usd` (`--trial-budget`, default 1.00) and `--max-turns` (default 40) as caps.
4. The harness reads the stream and checks the result itself through `tiza api` on the live instance — never the agent's word. A trial whose session did not list exactly one MCP server, `tiza`, as `connected` fails, and the run stops there.

The `tiza` binary is copied into the run directory before the first trial (with its commit, version and SHA-256 in `summary.json`, next to `claude --version`), so a rebuild in the worktree during a long run cannot change it between trials.

### Tasks

| task | fixture | the agent is asked to | the harness checks |
|---|---|---|---|
| `read` | `stack.rs` + `.tiza` (3 steps, open at step 2) | say the current step, the number of annotations and the function on line 4 | the answer's `STEP=`, `ANNOTATIONS=`, `FUNCTION=` against the document as it was opened (an agent may browse the steps before answering) |
| `box` | `stack.rs` | box the method `push` with a label, at step 1 | a box at step 1 spanning exactly `fn push` … its `}` with that label |
| `arrow-note` | `stack.rs` | at step 2, an arrow from `let max` to the `max` field, and a note on `let max` | the arrow's two lines and the note's line, both at step 2 |
| `redact` | `config.toml` with a fake token | mask the token's value, keep the rest readable | a redact exists; the token is in neither `code.get` nor `render.text`; the other lines still read |
| `move` | `stack.rs` | a line that appears at step 3, typed char by char | a `code.patch` at step 3 with `typing: char`; the line is right after its anchor at step 3 and absent at step 2 |
| `export` | `stack.rs` + `.tiza` (open at step 2) | a PNG of step 3 at `step3.png` | the file is byte for byte the bridge's own export of step 3 at the defaults; one that equals step 2 (the step on screen, `export_image`'s default) fails; one with other options passes only if the agent's successful `export_image` call named step 3 |
| `tour` | `main.rs` + `queue.rs` | box `main` at step 1; at step 2 switch to `queue.rs` and box `push` | a `code.link` to `queue.rs` at step 2, both boxes on the right file and lines, `check` without errors |
| `explain` | `stack.rs` | explain `parse_args` in four steps, free-form, with no errors | every step 1–4 brings something (an annotation or a move), nothing past step 4, at least one annotation over `parse_args` (its doc comment, its body, or a line that calls it; ranges count by overlap), `check` without errors |

An agent that never calls a Tiza tool fails, with its tokens recorded. `--break TASK` breaks that task's fixture on purpose (a renamed function the task names, a second copy of the token in a comment, a directory where the PNG should go): its check must then fail whatever the agent says. For most tasks the check fails on its own precondition (the function it looks for is gone), which is what test step 3 asks: the harness, not the agent, decides. `--self-check` proves the checks are not vacuous without calling a model: for every task the check fails on the untouched fixture, passes after a scripted solution (sent through the socket API the MCP tools call; for `export`, through `tiza mcp` itself over JSON-RPC, since where a relative path lands depends on the bridge), and fails again on the broken fixture; it also parses a recorded sample stream and a set of self-report lines.

### Variants

| variant | settings |
|---|---|
| `tool-search` | today's server, Claude Code's default: `ENABLE_TOOL_SEARCH` unset, MCP tools deferred and loaded through `ToolSearch` |
| `no-tool-search` | today's server with `ENABLE_TOOL_SEARCH=false`: every Tiza tool loaded upfront |

Later cards add their variant to `VARIANTS` in the script (t-162: server instructions and always-load; t-163: lean schemas) and re-run the same matrix with the same model.

### What each trial reports

Measured by the harness from the usage Claude Code returns (`stream-json`; requests that share a message id are counted once; output tokens come from the result message):

- **context at the first request** and **context at the first Tiza call** (the request that issued the first `mcp__tiza__*` call), each as fresh input + cache read + cache write — prompt caching changes the split between runs, so the three are kept apart;
- total input (fresh / cache read / cache write) and output tokens;
- Tiza tool calls, `ToolSearch` lookups, calls to any other tool (all denied: a wrong tool);
- tool errors, of which invalid arguments (a heuristic on the error text: `invalid_argument`, `text_not_found`, `out_of_range`, `invalid_params`, `InputValidationError`), and retries (a call of a tool that failed earlier in the trial; errors are matched to their call by `tool_use_id`);
- turns, wall time, the model, and Claude Code's `total_cost_usd`;
- success, checked by the harness;
- the agent's **self-report**: every prompt ends asking for a `CONTEXT: <number> tokens - <why>` line, and the table puts that number (the last such line; `12k`, `12,000`, `~1.2M` and bold all read) beside the measured one; the whole answer is kept with the trial.

Raw output goes to `scripts/agent-trials/runs/<stamp>/` (git-ignored): one `.jsonl` stream and one `.json` result per trial, `bin/tiza`, plus `summary.json` and `summary.md` (the table below). Ctrl-C or a SIGTERM stops after killing the running trial's whole process group, and still writes the summary of the trials done.

## How to run it

```sh
scripts/agent-trials/run --dry-run --model sonnet --max-trials 16   # the plan and the estimate; nothing runs
scripts/agent-trials/run --self-check                               # the checks against scripted solutions; no model
scripts/agent-trials/run --model sonnet --max-trials 16             # a real run: prints the estimate and asks first
scripts/agent-trials/run --model sonnet --tasks box --repeats 1 --break box   # test step 3: must report a failure
```

The plan is repeats × tasks × variants, in that order, so `--max-trials 16` is one pass of the 8 tasks under both variants, and the default `--repeats 2` is 32 trials. Tool search may load tools differently each time, so a cell is only trustworthy after two runs: 16 trials are a first look, 32 the baseline proper. Compare variants only within one model.

## Cost

The estimate printed before a run assumes about ten requests over a ~30k-token context per trial (20k fresh input, 40k cache writes, 300k cache reads, 4k output) at the Claude API's list prices:

| model | per trial | 16 trials | 32 trials |
|---|---|---|---|
| Sonnet 5.5 | ~$0.24 | ~$3.84 | ~$7.68 |
| Opus 5.5 | ~$0.42 | ~$6.72 | ~$13.44 |
| Fable 5.1 | ~$0.98 | ~$15.60 | ~$31.20 |
| Haiku 5.5 | ~$0.01 | ~$0.19 | ~$0.38 |

Each trial is also capped at `--trial-budget` dollars (default 1.00; raise it for Fable, whose estimate is close to that cap). The measured figure is Claude Code's own `total_cost_usd`, summed at the end of the run and kept per trial.

## Results

**Baseline: not run yet.** The harness, the dry run and the self-check are done (t-161); the first real run — `--max-trials 16`, with the model Luis picks — waits for his OK, since it spends his tokens. Its table goes here, with the model, the date, the commit and the run's cost, and the two variants side by side per task.
