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
   - `--tools ToolSearch` (`--builtin-tools`): tool search is the only built-in tool, so the agent cannot read files or run commands. `--tools ""` removes `ToolSearch` as well (the first baseline attempt found it: the tool-search variant then loaded every Tiza tool upfront), and the harness stops the run when the tool-search variant lacks it;
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

The estimate printed before a run assumes, per trial, 1k fresh input, 8k cache writes, 80k cache reads and 1.5k output — the first baseline's averages (4.4k writes, 46k reads, 0.8k output) with some headroom — at list input and output prices, cache writes at 2× input (Claude Code writes the 1-hour cache) and cache reads at the rate its `total_cost_usd` showed for Sonnet 5.5 ($0.10/M; the other models' read prices are the API table's).

| model | per trial | 16 trials | 32 trials |
|---|---|---|---|
| Sonnet 5.5 | ~$0.06 | ~$0.91 | ~$1.82 |
| Opus 5.5 | ~$0.11 | ~$1.82 | ~$3.65 |
| Fable 5.1 | ~$0.27 | ~$4.24 | ~$8.48 |
| Haiku 5.5 | ~$0.003 | ~$0.05 | ~$0.10 |

Each trial is also capped at `--trial-budget` dollars (default 1.00). The measured figure is Claude Code's own `total_cost_usd`, summed at the end of the run and kept per trial. The first baseline cost $0.48 for 16 trials with Sonnet 5.5, plus $0.10 for the two probe trials that found the `--tools ""` problem.

## Results

### Baseline, 2026-10-18 (Sonnet 5.5, 16 trials, one pass)

`claude-sonnet-5-5`, Claude Code 2.1.303, `tiza` at 472e224 (the run's `+dirty` is the harness's own `--builtin-tools` default, changed between the probe and this run; the binary is 472e224's). One pass of each cell, so treat a single cell as a first look; the corner case asks for two runs per cell (`--max-trials 32`). Raw streams outside the repo, in `pruebas/t161/20261018-191431/` on Luis's Mac. Context figures are tokens of one request (fresh input + cache read + cache write); "input" is the sum over every request of the trial.

| task | variant | ok | ctx 1st request | ctx at 1st tiza call | ctx peak | input total | of which cache writes | tiza calls | ToolSearch | errors | turns | cost $ | self-report |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| read | tool search | yes | 4,041 | 4,810 | 5,934 | 14,785 | 3,120 | 3 | 1 | 0 | 5 | 0.016 | 9,000 |
| read | upfront | yes | 16,799 | 16,799 | 17,927 | 34,726 | 1,876 | 3 | — | 0 | 4 | 0.013 | 6,000 |
| box | tool search | yes | 4,002 | 5,395 | 6,294 | 21,735 | 3,880 | 2 | 1 | 0 | 4 | 0.021 | 9,000 |
| box | upfront | yes | 16,759 | 16,759 | 17,661 | 51,828 | 2,010 | 2 | — | 0 | 3 | 0.016 | 9,000 |
| arrow-note | tool search | **no** | 4,036 | 6,363 | 7,560 | 25,307 | 5,146 | 3 | 1 | 1 | 5 | 0.030 | 14,000 |
| arrow-note | upfront | yes | 16,792 | 16,792 | 18,121 | 52,354 | 2,470 | 3 | — | 0 | 4 | 0.021 | 9,000 |
| redact | tool search | yes | 3,999 | 5,614 | 6,980 | 29,215 | 4,566 | 5 | 1 | 0 | 7 | 0.027 | 9,000 |
| redact | upfront | yes | 16,755 | 16,755 | 17,761 | 69,395 | 2,110 | 4 | — | 0 | 5 | 0.019 | 6,000 |
| move | tool search | yes | 4,035 | 6,715 | 7,629 | 25,743 | 5,215 | 2 | 1 | 0 | 4 | 0.029 | 9,000 |
| move | upfront | yes | 16,791 | 16,791 | 17,705 | 51,936 | 2,054 | 2 | — | 0 | 3 | 0.017 | 9,000 |
| export | tool search | yes | 3,980 | 5,300 | 5,550 | 14,830 | 3,136 | 1 | 1 | 0 | 3 | 0.018 | 6,000 |
| export | upfront | yes | 16,735 | 16,735 | 16,953 | 33,688 | 1,302 | 1 | — | 0 | 2 | 0.011 | 2,500 |
| tour | tool search | yes | 4,039 | 6,278 | 14,788 | 122,585 | 12,374 | 14 | 3 | 1 | 18 | 0.080 | 30,000 |
| tour | upfront | yes | 16,797 | 16,797 | 18,554 | 88,837 | 2,903 | 7 | — | 0 | 8 | 0.027 | 11,000 |
| explain | tool search | yes | 4,013 | 10,919 | 14,566 | 55,810 | 12,152 | 11 | 2 | 0 | 14 | 0.076 | 21,000 |
| explain | upfront | yes | 16,772 | 16,772 | 21,838 | 120,058 | 6,187 | 17 | — | 0 | 18 | 0.059 | 22,000 |

Totals per variant: tool search 310,010 input tokens (49,589 cache writes), 7,126 output, $0.296, 89 s; upfront 502,822 input (20,912 cache writes), 5,016 output, $0.182, 67 s. No trial called a tool other than Tiza's and tool search, and none read a file.

What it says:

- **The Tiza tool list costs about 12,700 tokens of context when it is loaded upfront**: the first request of the upfront variant is ~16,800 tokens against ~4,000 with tool search (net of tool search's own definition and its list of deferred names: ~410 tokens per tool for 31 tools). With tool search, the context when the agent makes its first Tiza call is 4,800–6,700 tokens for single-purpose tasks and 10,900 for `explain`, and the peak stays under 15,000 even for the longest tasks.
- **Smaller context did not mean cheaper here.** Tool search used 38 % fewer input tokens in all but cost 62 % more ($0.296 against $0.182). Claude Code's `total_cost_usd` fits Sonnet 5.5 at $4/M for a cache write (every write in the streams is a 1-hour cache write, 2× the input price), $0.10/M for a cache read (0.05×) and $10/M for output: a write costs 40 reads. What tool search loads changes the prompt and is written to the cache; the upfront list is a stable prefix that is read. That prefix is shared by every trial through the API's prompt cache (it lasts an hour and spans processes and runs), and the probe run that stopped a minute earlier had already written it, so the upfront trials never paid for it (trial 2, the first upfront one, read 16,049 tokens from the cache on its first request). Counting that write once, the run would have cost about $0.307 with tool search against $0.245 upfront: tool search ~25 % dearer, not 62 %. Of the measured gap, cache writes account for +$0.115, output +$0.021 and reads −$0.022. Whether a long session reverses it (the saving is paid on every request; a warm upfront prefix costs ~$0.0013 per request to read) was not measured: a later run with a multi-task session would tell.
- **`tour` took 18 turns and 14 Tiza calls with tool search against 8 and 7 upfront**, the one task where it used more input in total, for two reasons. The agent boxed `main` with `find_start "fn main"` / `find_end "}"`, and the first `}` after it closes the `for` loop, so the box came out L5-9 (the agent said so); the upfront agent used line numbers. Then it called `update_annotation` with no schema loaded (`missing field id`), searched again, and deleted and re-added the box. Tool search's own share is that call and two extra lookups; the `find_end` ambiguity is a usability finding of its own (the agent guide warns about it, the tool descriptions do not). One trial, so neither is a rule yet.
- **The one failure is a usability finding, not a harness one.** In `arrow-note` with tool search the agent called `goto_step 2` on a document with one step, got `step 2 is beyond the last step (1)`, concluded that it could not draw at step 2 and asked the user instead of adding the annotations with `step: 2` (which creates the step). The upfront agent did it right. Input for t-162: the instructions should say that annotating at step N creates step N.
- **Self-reports are not a measure.** The agents' own estimates (2,500–30,000) mostly overshoot the peak with tool search (9,000 against ~6,000) and undershoot it upfront (6,000–9,000 against ~17,000; `explain` upfront, 22,000 against 21,838, is the exception), and against the total input every one undershoots, by 1.6–5.8×: an agent does not see what its tool list costs. The measured figures are the ones to compare.
- Success 15/16; tool errors 2 (both counted as invalid arguments); no retries of a failed call, no wrong tools.

Next: a second pass of every cell (`--max-trials 16` or `--repeats 1` again; `--max-trials 32` would be two more) before drawing conclusions per task — run with a warm cache for both variants, or note which one it favours — then the same matrix again with each change of t-162 and t-163. Usability findings for t-162: annotating at step N creates step N (the `goto_step` dead end), and `find_end` takes the first match after the start, which for `}` is often an inner block.
