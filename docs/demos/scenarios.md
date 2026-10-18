# Demo scenarios: ways to use Tiza with an agent

Twelve concrete situations where Tiza and an agent (Claude Code, or any MCP client) explain code to someone: a viewer, a new teammate, a reviewer, or another agent. Each one is an idea for a recording and a check that the pieces exist. Scenario 1 is worked out in full as a runnable example ([below](#worked-example-a-tour-of-tally)); the others are scripts to adapt.

Every command, tool and field named here exists today (README, [agent guide](../agent-guide.md)). What a scenario would need and Tiza does not do yet is in the [list at the end](#what-each-scenario-needs-that-tiza-does-not-do-yet), not in the scripts. Any frame you publish from these is a real `tiza render` export; do not retouch it.

Each scenario gives: **For** (the audience), **Setup** (a sample under `examples/`, or how to make a throwaway one), **Script** (the prompts to the agent and the `tiza` or MCP calls), **What the audience sees**, **Features** and **Recording** (format and length).

| # | Scenario | For | Sample |
|---|---|---|---|
| 1 | [A codebase tour across a few files](#1-a-codebase-tour-across-a-few-files) | viewer, new teammate | `examples/tally/` (worked example) |
| 2 | [One agent explains a change to another for review](#2-one-agent-explains-a-change-to-another-for-review) | reviewer agent, viewer | `examples/tally/` |
| 3 | [A walkthrough one agent leaves for the next](#3-a-walkthrough-one-agent-leaves-for-the-next) | the next agent or teammate | `examples/tally-tour.tiza` |
| 4 | [Walking through a change, before and after](#4-walking-through-a-change-before-and-after) | reviewer, viewer | `examples/ownership-moves.rs.tiza` |
| 5 | [Tracing a bug across files to its fix](#5-tracing-a-bug-across-files-to-its-fix) | viewer, teammate | `examples/tally/` |
| 6 | [Explaining a config file without leaking its secrets](#6-explaining-a-config-file-without-leaking-its-secrets) | viewer, teammate | a throwaway `.env` |
| 7 | [Live coding: the agent annotates while you edit](#7-live-coding-the-agent-annotates-while-you-edit) | viewer | a copy of `examples/tally/` |
| 8 | [Supervised session: the agent proposes, you approve on camera](#8-supervised-session-the-agent-proposes-you-approve-on-camera) | viewer | `examples/src/stack.rs` |
| 9 | [One concept, step by step](#9-one-concept-step-by-step) | learner | `examples/ownership.rs` |
| 10 | [Same shape, other end: two implementations](#10-same-shape-other-end-two-implementations) | learner | `examples/stack-and-queue.tiza` |
| 11 | [Answering a question with an annotated frame](#11-answering-a-question-with-an-annotated-frame) | whoever asked | `examples/tally/src/count.rs` |
| 12 | [Hands-free: a looping screen or an unattended take](#12-hands-free-a-looping-screen-or-an-unattended-take) | passers-by, viewer | `examples/tally-tour.tiza` |

Common setup for every scenario with an agent: Tiza in one terminal, the agent in another, and the MCP server registered once (`claude mcp add tiza -- tiza mcp`, or the `.mcp.json` in the README). Before any take, `tiza check FILE` (or the `check` tool) and a look at `redact_alerts` in `status`.

## 1. A codebase tour across a few files

**For:** someone who has never seen the code — a viewer, a new teammate, or you on your first day in a repository.

**Setup:** `examples/tally/`, a real four-file Rust CLI (a tiny `wc`: `main.rs` wires `args.rs`, `count.rs` and `report.rs`; `cargo run -- --by-words src/*.rs` inside it works). Open its entry point: `tiza examples/tally/src/main.rs --no-save` in a terminal at least 130 columns wide (`--no-save` keeps the sidecar out of the repository).

**Script:** you ask the agent to show you the program file by file. It reads `get_code`, links each file at its own step with `add_move {action: "code.link"}`, draws on each one while it is shown, and comes back to `main.rs` for a summary. The exact prompts are in the [worked example](#worked-example-a-tour-of-tally).

**What the audience sees:** step 1, `main.rs` with its three calls numbered as a flow (parse, count, report) and the error path noted; step 2, `args.rs` alone with its loop boxed and the error marked; step 3, `count.rs` with a callout on why bytes come free; step 4, `report.rs` with the sort boxed; step 5, `main.rs` again, its step-1 annotations exactly as they were, the `report::table` call selected, a callout that sums the flow up and everything else dimmed.

**Features:** `code.link` views with their own annotations, `lines` windows, named steps, `select`, spotlight, callouts, `check`, the `visible` reply of every `add_*` (t-155).

**Recording:** a screen recording of the terminal and the agent side by side while it draws (2-4 min), or the finished take replayed: `tiza examples/tally-tour.tiza --no-save --present` and `n`. Native GIF of the finished take: `tiza render examples/tally-tour.tiza --out tally-tour.gif --width 130 --interval 3` (6 frames, about 440 kB).

### Worked example: a tour of tally

`examples/tally-tour.tiza` is the take the prompts below produce: a document over `examples/tally/src/main.rs` with four `code.link` moves (to `args.rs` lines 14-30, `count.rs`, `report.rs`, and back to `main.rs` lines 9-28), one `select`, five named steps (`main`, `args`, `count`, `report`, `recap`) and 16 annotations. `tiza check examples/tally-tour.tiza --width 130` reports 0 errors and 0 warnings. `qa/tests/t-046.rs` replays the tool calls these prompts lead to on a live instance and checks that every step draws exactly as the shipped file.

Open the bare file, with the agent connected:

```sh
tiza examples/tally/src/main.rs --no-save
```

Then, to Claude Code (one prompt at a time; wait for each to finish):

1. `Use the tiza MCP server. Call status and get_code: this is main.rs of tally, a small CLI in examples/tally/src. Without drawing anything, tell me in three lines what the program does and which files it uses.`
2. `Go to step 1 (next_step) so I see it being drawn. On main.rs at step 1: add_mark kind flow on args::parse at line 10, label "1 parse the flags", color blue, id m1; the same on count::count at line 21, "2 count each file", green, id m2; and on report::table at line 26, "3 print the table", purple, id m3. Then add_note on line 14: "a bad flag ends here: exit code 2" (id n1), and add_box over lines 19-24 labelled "a file it cannot read is skipped" (id b1). Anchor with find, never with columns.`
3. `Now show me args.rs: add_move at step 2 with action code.link, file "args.rs" (relative to main.rs's folder), lines "14-30", then next_step. At step 2: add_box lines 17-25 "every argument is a path, except the flag" in blue (id b2); add_mark kind error on Err at line 21, "main prints it, exits 2" (id k1); add_note on line 26 "no paths at all is an error too" (id n2).`
4. `Next, count.rs: add_move at step 3, code.link to "count.rs" (the whole file), then next_step. At step 3: add_callout lines 11-14, title "bytes come free", text "text.len() is the byte length of the String, so only lines and words need the loop.", cyan (id c1); add_box lines 15-18 "one pass over the lines", green (id b3); highlight split_whitespace() on line 17 with label "runs of spaces count once" (id h1).`
5. `Then report.rs: add_move at step 4, code.link to "report.rs", then next_step. At step 4: add_mark kind new on rows.to_vec() at line 7, "a copy: the caller's rows are untouched" (id k2); add_box lines 8-11 "--by-words: most words first", purple (id b4); add_note on line 14 "right-aligned columns, like wc" (id n3).`
6. `Back to main.rs for the recap: add_move at step 5, code.link to "main.rs" with lines "9-28", and a second move at step 5, action select, from {find: "report::table"} to {find: "opts.sort)"}; then next_step. At step 5: add_callout lines 18-26, placement auto, title "the whole flow", text "args.rs turns the flags into Options, count.rs reads each file once, report.rs sorts and prints. main only wires them together.", cyan (id c2); add_spotlight at step 5 with ids m1, m2, m3 and c2 (id s1).`
7. `Name the steps. There is no MCP tool for that, so run this in the shell: tiza scene patch '{"steps":[{"step":1,"id":"main","title":"main.rs: parse, count, report"},{"step":2,"id":"args","title":"args.rs: one flag, by hand"},{"step":3,"id":"count","title":"count.rs: one pass"},{"step":4,"id":"report","title":"report.rs: sort, then print"},{"step":5,"id":"recap","title":"Back in main.rs"}]}'`
8. `Call check with width 130 and fix anything it reports. Then goto_step "count" to show me it works, and set_step 0.`

Now present it: `P` for presentation mode and `n` through the steps (or `tiza next` / `tiza goto recap` from another terminal). Each `add_*` reply says whether the annotation is on screen (`m1 added at step 1; visible now …`); if the agent ever answers `not visible now`, it says why, and the take is still right. The agent may phrase things differently or pick other columns; `examples/tally-tour.tiza` is the reference result, and `tiza examples/tally-tour.tiza --no-save` replays it without an agent.

To keep the agent's version, the document must come out of the live instance by hand (see the list at the end): `tiza scene get` prints an envelope whose `document` is the take with every file embedded. For the recording itself, render the shipped file rather than the live instance (the live `render` of a document with links is the bug listed at the end):

```sh
tiza render examples/tally-tour.tiza --out tally-tour.gif --width 130 --interval 3
tiza render examples/tally-tour.tiza --all-steps --width 130 --out tally-tour-steps/
```

## 2. One agent explains a change to another for review

**For:** a reviewing agent, and the person watching two agents talk through code — a review, or knowledge passing between a coding agent and a checking one.

**Setup:** `examples/tally/` copied somewhere writable (`cp -r examples/tally /tmp/tally`), Tiza on `tiza /tmp/tally/src/args.rs --no-save`. Two agent sessions (two Claude Code terminals) with the same `tiza` MCP server registered: each starts its own `tiza mcp`, and both talk to the one instance (the socket serves several clients at once). Optionally `:supervise on`, so every change from either agent waits for your `y`/`n`.

**Script:**
- To agent A (the author): `You want to add a --by-lines flag to tally. Explain the change on Tiza instead of editing the file: at step 1 box the current flag handling; at step 2 add the new branch as a code.patch move (op insert after the line with "sort = Sort::Words;", typing "line"), and annotate the new lines at step 2. Prefix every id with "a-".`
- To agent B (the reviewer): `Read tiza://scene and tiza://render.md (MCP resources), then list_annotations. Review the change shown at step 2. Put your findings at step 3 as marks of kind question or warn, or notes, with ids prefixed "rev-". Do not touch annotations you did not write.`
- To agent A again: `list_annotations: answer each rev- annotation at step 4 with a note of your own, or fix your move (delete_move, add_move) and say so.`

**What the audience sees:** the flag loop boxed; at step 2 the new branch typing itself in, with the author's notes; at step 3 the reviewer's questions and warnings appear on the same lines; at step 4 the answers. `p` walks it back, and the typed code goes away again.

**Features:** several MCP clients on one instance, `code.patch` with `typing`, the `question` and `warn` kinds, the `tiza://` resources, `list_annotations`, `delete_move`, `:supervise`.

**Recording:** three panes (agent A, agent B, Tiza), 3-5 min; afterwards a GIF of the finished document (render it from a saved file, see scenario 3).

## 3. A walkthrough one agent leaves for the next

**For:** the next agent (or teammate) who opens this code tomorrow without the conversation that explained it.

**Setup:** a finished take such as `examples/tally-tour.tiza`, or the live result of scenario 1.

**Script:**
- Agent A, after a tour like scenario 1, saves the live document as a self-contained file: `tiza scene get > tour.json`, then keep its `document` field as `walkthrough.tiza` (`python3 -c 'import json,sys; json.dump(json.load(sys.stdin)["document"], sys.stdout)' < tour.json > walkthrough.tiza`). `scene get` always embeds the code and every linked file under `views`, so the file still opens after the code moves on.
- Agent B, in a new session and with no TUI: `tiza check walkthrough.tiza`, then `tiza render walkthrough.tiza --all-steps --format md --out walkthrough-md/` and read `step-01.md` … in order. Or open it in an instance of its own (`tiza walkthrough.tiza --session b`, with `tiza mcp --session b`) and read `tiza://render.md` after each `goto_step`.
- To agent B: `Read the walkthrough step by step and answer: where would a --by-lines flag go, and which files change?`

**What the audience sees:** mostly the second agent's answer, quoting frames of a tour it never watched; as a video, the Markdown frames scrolling in a terminal or an editor.

**Features:** `scene get` (embedded code and `views`), `tiza check`, `render --all-steps --format md`, `--session`, `goto_step`, the `tiza://` resources.

**Recording:** better as a written example (the frames are text) than as a video; under a minute if recorded.

## 4. Walking through a change, before and after

**For:** a reviewer reading a pull request, or a viewer watching a refactor.

**Setup:** `examples/ownership-moves.rs.tiza`, where step 2 types a borrow and a `len` helper into the code and step 4 replaces the call with `t.len()` and deletes the helper. For your own change, any file and its `git diff`.

**Script:** `Read git diff for this file. Reproduce each hunk as an add_move code.patch at its own step (insert, replace or delete, typing "line"), oldest hunk first; annotate the new code at the step where it exists; if an annotation goes (unanchored) after a replace, give the move a rebind. Then call check.`

**What the audience sees:** the old code, then each change typing itself in with a note on why; going back (`p`) undoes it line for line.

**Features:** `code.patch` (`after`, `find`, `occurrence`), `typing` and `speed_ms`, `rebind`, `(unanchored)` in `list_moves`, `check` on move targets.

**Recording:** the native GIF keeps the typed frames (`tiza render examples/ownership-moves.rs.tiza --out moves.gif`); `--out moves.tape` and `vhs` for a terminal-window look; 1-2 min.

## 5. Tracing a bug across files to its fix

**For:** a viewer who likes debugging stories; a teammate who hit the bug.

**Setup:** `examples/tally/` copied to `/tmp/tally`. A real limitation of tally: a file whose name starts with `--` cannot be counted (`tally --notes.txt` says `unknown flag --notes.txt`), because `args.rs` treats any `--` prefix as a flag and there is no `--` separator. `tiza /tmp/tally/src/main.rs --no-save`.

**Script:** `A user reports: tally --notes.txt says "unknown flag". Trace it on Tiza from the symptom to the cause: step 1, the eprintln in main.rs that prints it; step 2, link args.rs and mark the line that decides it; step 3, spotlight only that branch; step 4, propose the fix as a code.patch typed into args.rs (accept a lone "--" that ends the flags) and annotate it.`

**What the audience sees:** the error message boxed in `main.rs`, the jump to `args.rs` with an `error` mark on `starts_with("--")`, the rest of the file dimmed, then the fix typing itself in.

**Features:** `code.link`, `error` marks, spotlight, `code.patch` on a linked file, `check`.

**Recording:** 2-3 min live; the GIF of the finished take for a post.

## 6. Explaining a config file without leaking its secrets

**For:** a viewer, or a teammate setting up the project; any take where a real token could appear.

**Setup:** examples ship no secrets on purpose; make a throwaway file with fake values: `printf 'DATABASE_URL=postgres://app:hunter2@db.local/app\nAPI_TOKEN=tok_demo_0000000000000000\nLOG_LEVEL=info\n' > /tmp/demo.env`, then `tiza /tmp/demo.env --no-save`.

**Script:**
- You, in the TUI: `:redact auto --dry` to see what it would mask, then `:redact auto` (it masks the password in the URL and the token value as two ordinary `redact` annotations you can review in `:list`).
- To the agent: `Call status and check redact_alerts is empty, then get_code. Explain each setting with a note on its line. Never quote a masked value; if you spot a secret that is not masked, add a redact with at.find set to the text before it.`

**What the audience sees:** `••••••••` where the password and the token are, on screen and in every export, with a note per line; `◆ redacted` on the bar in normal mode.

**Features:** `:redact auto`, the MCP `redact` tool, masks in `get_code` and the resources, `redact_alerts` in `status` (t-156), `:redact off` (only the author's own screen).

**Recording:** presentation mode; before recording, read the screen (`auto` only knows the listed shapes) and run `tiza check`. Under a minute.

## 7. Live coding: the agent annotates while you edit

**For:** a viewer watching code being written, with the explanation keeping up.

**Setup:** a copy of `examples/tally/` (`cp -r examples/tally /tmp/tally`), open in the editor and in `tiza /tmp/tally/src/args.rs --follow`. Zed integration: `docs/zed.md`.

**Script:** you add a `--by-lines` flag in the editor and save after each piece; after each save, to the agent: `list_annotations: re-add what the reload dropped, and mark the lines I just added.` Or ask it once to `set_follow` on and keep the annotations up to date.

**What the audience sees:** the editor and Tiza side by side; annotations follow their text as lines move, the ones whose text is gone disappear, and the agent puts new ones on the new code.

**Features:** follow mode and re-anchoring, `set_follow`, `follow.status` (`reloads`, `last_reload`), `list_annotations`, `u` to undo a reload.

**Recording:** a screen recording, 3-5 min; the editor at 50 % and Tiza at 50 % (README «Recommended recording profile»).

## 8. Supervised session: the agent proposes, you approve on camera

**For:** a viewer who wants to see the human stay in control of what an agent draws.

**Setup:** `examples/src/stack.rs`, bare: `tiza examples/src/stack.rs --no-save`, then `:supervise on`. (`examples/stack-impl.tiza` is a finished take of the same lesson.)

**Script:** `Explain why push and pop take &mut self and peek takes &self: one mark per method, a note on Option<&T>.` Each call waits; you press `y` for one, `n` for another, `a` for the rest.

**What the audience sees:** the review overlay listing each proposal (`▶ p1  add note L4 «…»`), `⏸ N to review` on the bar, the accepted ones appearing, and the agent saying what it wanted after a rejection instead of retrying.

**Features:** `:supervise`, the review overlay, `rejected`, the presentation-mode chip, `status.pending`.

**Recording:** normal mode (the overlay is the point), 1-2 min.

## 9. One concept, step by step

**For:** someone learning one idea — the use Tiza was born for (Rust ownership, in tutorials).

**Setup:** `examples/ownership.rs`, open with `tiza examples/ownership.rs --no-save`; the finished lesson is `examples/ownership.rs.tiza` (7 steps).

**Script:** `Explain move, shared borrow and &mut on this file in seven steps, one colour per value: arrows solid for a move, dashed for a shared borrow, thick for &mut; a callout that sums up at the end. Use a style borrow_mut with define_style.`

**What the audience sees:** the README's ownership take: `s` moves, `t` is borrowed, `u` is borrowed mutably, `takes` drops `s`.

**Features:** marks and kinds, `define_style`, arrows with `stroke`/`heads`, callouts, spotlight, stable geometry across steps, legibility after compression.

**Recording:** presentation mode at 108 columns or more (`tiza check` says so), or the native GIF the README uses; a 5-10 min tutorial segment.

## 10. Same shape, other end: two implementations

**For:** a learner comparing two designs side by side in time.

**Setup:** `examples/stack-and-queue.tiza` over `examples/src/stack.rs` and `examples/src/queue.rs`.

**Script:** `Walk me through the stack and the queue: box push/pop in stack.rs, link queue.rs and box enqueue/dequeue, select push_back to pop_front and pause, then link back to stack.rs and show that both peek signatures borrow.`

**What the audience sees:** each file with its own boxes and notes; going back to `stack.rs` brings its annotations back as they were; a narrower window of `queue.rs` at the end.

**Features:** `code.link` back and forth, `lines` windows, `select`, `cursor`, `wait`.

**Recording:** the native GIF honours the `wait` (`tiza render examples/stack-and-queue.tiza --out sq.gif`); under a minute.

## 11. Answering a question with an annotated frame

**For:** whoever asked in an issue, a pull request, a chat or a doc — no video at all.

**Setup:** any file, for example `examples/tally/src/count.rs` (`tiza examples/tally/src/count.rs --no-save`).

**Script:** `Someone asks why tally counts "a   b" as two words. Answer with one annotated frame: highlight split_whitespace() and add a note, then export_image to /tmp/answer.md (and /tmp/answer.png for the issue).`

**What the audience sees:** a fenced text block (or a PNG) with the line highlighted and the note beside it, pasted under the question.

**Features:** `export_image` to `.md`, `.txt` or `.png`, `render --all-steps` for a short series, masks applied on every export.

**Recording:** a still, not a video; `tiza render FILE --out answer.md --step 1` does the same from the shell.

## 12. Hands-free: a looping screen or an unattended take

**For:** passers-by at a booth or a meetup screen; a recording with no one pressing keys.

**Setup:** `examples/tally-tour.tiza`, opened with `tiza examples/tally-tour.tiza --no-save --present` (nothing is written back).

**Script:** from another terminal, give every step a duration and loop it: `tiza scene patch '{"steps":[{"step":1,"id":"main","title":"main.rs: parse, count, report","auto_advance_ms":4000},{"step":2,"id":"args","title":"args.rs: one flag, by hand","auto_advance_ms":4000},{"step":3,"id":"count","title":"count.rs: one pass","auto_advance_ms":4000},{"step":4,"id":"report","title":"report.rs: sort, then print","auto_advance_ms":4000},{"step":5,"id":"recap","title":"Back in main.rs","auto_advance_ms":6000}],"loop":true}'` (the list replaces the old one). For an editor-driven take instead, bind `tiza next`, `tiza prev` and `tiza goto recap` to keys (`docs/zed.md`).

**What the audience sees:** the tour playing by itself, step 5 back to step 0, forever.

**Features:** `auto_advance_ms`, `loop`, presentation mode, `tiza next|prev|goto`; the native GIF and MP4 hold such a step for exactly its `auto_advance_ms`.

**Recording:** none needed for the booth. For a video, the native GIF of a document whose `steps` carry these durations holds each step exactly that long.

## What each scenario needs that Tiza does not do yet

Gaps found while writing these scripts, for Luis to turn into cards or not. Numbers are the scenarios that hit them.

- **Bug: rendering the live instance of a multi-file document** (1, 2, 3, 10, 12). `tiza render --out x.gif` with no FILE builds the document from `scene.get` without loading its linked files: on `tiza examples/tally/src/main.rs` with a `code.link` move it panics (`index out of bounds` in `crates/tiza-core/src/layout.rs`), and `--out - --format md --step 2` shows `main.rs` where the instance shows `args.rs`. The MCP `export_image` builds its scene the same way (read in the code, not run). Workaround: render the `.tiza` file, never the live instance, when a document has links.
- **Arrows between files, or two files on screen at once** (1, 5, 10). An arrow lives in one view; a tour cannot point from the `args::parse` call in `main.rs` to `fn parse` in `args.rs`, and there is no split view.
- **No MCP tool for the document's own fields** (1, 12). Named steps, `auto_advance_ms`, `loop`, `title`, `margin` go through `scene.patch`, which the MCP does not expose; the agent shells out to `tiza scene patch`.
- **No way to save the live document as a `.tiza`** (1, 3). There is no `save` method on the socket or the MCP; `tiza scene get` prints an envelope that `tiza check` and `tiza render` refuse (`not a .tiza document`), so its `document` field has to be extracted by hand. `:w PATH` is the presenter's.
- **Who drew what** (2, 3). Annotations carry no author or origin; two agents on one instance need an id-prefix convention, and a reviewer's note cannot point at the annotation it answers (no replies or threads).
- **No change notifications for the resources** (2, 7). The server does not support MCP resource subscriptions; a second agent, or an agent following the presenter's saves, has to poll `tiza://scene` or `list_annotations`.
- **Reading another step without moving the presenter's view** (2, 3). `render_text` and the `tiza://` resources show the current step only; `export_image {step}` to `.md` works on one file but hits the bug above when the document has links.
- **No import of a diff as moves** (4, 5). Each `code.patch` is written by hand from `git diff`.
- **`:redact auto` is the presenter's only** (6). There is no socket or MCP method for it; an agent can add one `redact` per value it sees. Possibly right as it is (opt-in by the person on camera); listed in case a scenario needs it.
- **The Zed tasks for `tiza next|prev|goto` have not been run in Zed** (7, 12): `docs/zed.md` says so.
