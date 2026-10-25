# Demo scenarios: ways to use Tiza with an agent

Twelve concrete situations where Tiza and an agent (Claude Code, or any MCP client) explain code to someone: a viewer, a new teammate, a reviewer, or another agent. Each one is an idea for a recording and a check that the pieces exist. Scenario 1 is worked out in full as a runnable example ([below](#worked-example-a-tour-of-tally)); the others are scripts to adapt.

Every command, tool and field named here exists today ([`:` commands](../reference/commands.md), [agent guide](../agent-guide.md)), and no script waits on a feature: the one planned change that touches them is in the [short list at the end](#planned-changes-that-touch-these-scenarios). Any frame you publish from these is a real `tiza render` export; do not retouch it.

Each scenario gives: **For** (the audience), **Setup** (a sample under `examples/`, or how to make a throwaway one), **Script** (the prompts to the agent and the `tiza` or MCP calls), **What the audience sees**, **Features** and **Recording** (format and length).

| # | Scenario | For | Sample |
|---|---|---|---|
| 1 | [A codebase tour across a few files](#1-a-codebase-tour-across-a-few-files) | viewer, new teammate | `examples/tally/` (worked example) |
| 2 | [One agent explains a change to another for review](#2-one-agent-explains-a-change-to-another-for-review) | reviewer agent, viewer | `examples/tally/` |
| 3 | [A walkthrough one agent leaves for the next](#3-a-walkthrough-one-agent-leaves-for-the-next) | the next agent or teammate | the sidecar of scenario 1 |
| 4 | [Walking through a change, before and after](#4-walking-through-a-change-before-and-after) | reviewer, viewer | `examples/ownership-moves.rs.tiza` |
| 5 | [Tracing a bug across files to its fix](#5-tracing-a-bug-across-files-to-its-fix) | viewer, teammate | `examples/tally/` |
| 6 | [Explaining a config file with demo values](#6-explaining-a-config-file-with-demo-values) | viewer, teammate | a throwaway `.env` |
| 7 | [Live coding: the agent annotates while you edit](#7-live-coding-the-agent-annotates-while-you-edit) | viewer | `examples/tally/` |
| 8 | [Supervised session: the agent proposes, you approve on camera](#8-supervised-session-the-agent-proposes-you-approve-on-camera) | viewer | `examples/src/stack.rs` |
| 9 | [One concept, step by step](#9-one-concept-step-by-step) | learner | `examples/ownership.rs` |
| 10 | [Same shape, other end: two implementations](#10-same-shape-other-end-two-implementations) | learner | `examples/stack-and-queue.tiza` |
| 11 | [Answering a question with an annotated frame](#11-answering-a-question-with-an-annotated-frame) | whoever asked | `examples/tally/src/count.rs` |
| 12 | [Hands-free: a looping screen or an unattended take](#12-hands-free-a-looping-screen-or-an-unattended-take) | passers-by, viewer | `examples/tally-tour.tiza` |

Common setup for every scenario with an agent: Tiza in one terminal, the agent in another, and the MCP server registered once (`claude mcp add tiza -- tiza mcp`, or the `.mcp.json` in [Connect an agent](../agents/setup.md)). Before any take, `tiza check FILE` (or the `check` tool) and a look at the screen.

**Work on a copy, and keep what Tiza saves.** Copy the sample to a working folder first (`rm -rf /tmp/tally && cp -r examples/tally /tmp/tally`) and open the copy normally. Start from a fresh copy every time: `cp -r` onto a folder that already exists copies into `/tmp/tally/tally` instead, and you would open the previous scenario's sidecar. The TUI writes every change to the `.tiza` sidecar next to the file it opened (`/tmp/tally/src/main.rs` → `/tmp/tally/src/main.rs.tiza`) as it happens, the agent's changes included: that sidecar *is* the result. Keep it, render it, or hand it to another agent; it references the copied files by relative path, so move the whole folder with it (or run `:embed` in the TUI to store every file inside it). Opening the copy, never `examples/` itself, keeps the repository clean.

## 1. A codebase tour across a few files

**For:** someone who has never seen the code — a viewer, a new teammate, or you on your first day in a repository.

**Setup:** `examples/tally/`, a real four-file Rust CLI (a tiny `wc`: `main.rs` wires `args.rs`, `count.rs` and `report.rs`; `cargo run -- --by-words src/*.rs` inside it works). Copy it and open its entry point: `rm -rf /tmp/tally && cp -r examples/tally /tmp/tally`, then `tiza /tmp/tally/src/main.rs` in a terminal 130 columns wide (the width the take is checked and rendered at, so the code's long lines fit; the labels alone fit from 55).

**Script:** you ask the agent to show you the program file by file. One file is on screen at a time, and each file is explained at its own step: the agent reads `get_code`, links the file at its step with `add_move {action: "code.link"}`, draws on it while it is shown, and says in a label or a note where the flow goes next (`1 parse the flags` on the call in `main.rs`, then `args.rs` at step 2). It comes back to `main.rs` for a summary. The exact prompts are in the [worked example](#worked-example-a-tour-of-tally).

**What the audience sees:** step 1, `main.rs` with its three calls numbered as a flow (parse, count, report) and the error path noted; step 2, `args.rs` with its loop boxed and the error marked; step 3, `count.rs` with a callout on why bytes come free; step 4, `report.rs` with the sort boxed; step 5, `main.rs` again, its step-1 annotations exactly as they were, the `report::table` call selected, a callout that sums the flow up and everything else dimmed.

**Features:** `code.link` views with their own annotations, `lines` windows, named steps, `select`, spotlight, callouts, `check`, the `visible` reply of every `add_*`, autosave to the sidecar.

**Recording:** a screen recording of the terminal and the agent side by side while it draws (2-4 min), or the finished take replayed: `cp examples/tally-tour.tiza /tmp/` (its code points at `tally/src/main.rs`, which resolves to the copy in `/tmp/tally`), `tiza /tmp/tally-tour.tiza --present` and `n`. Native GIF of the finished take: `tiza render examples/tally-tour.tiza --out tally-tour.gif --width 130 --interval 3` (6 frames, about 440 kB; `render` never writes the document).

### Worked example: a tour of tally

`examples/tally-tour.tiza` is the take the prompts below produce: a document over `examples/tally/src/main.rs` with four `code.link` moves (to `args.rs` lines 14-30, `count.rs`, `report.rs`, and back to `main.rs` lines 9-28), one `select`, five named steps (`main`, `args`, `count`, `report`, `recap`) and 16 annotations. `tiza check examples/tally-tour.tiza --width 130` reports 0 errors and 0 warnings. A QA test replays the tool calls these prompts lead to on a live instance and checks that every step of the sidecar it autosaves draws exactly as the shipped file.

Copy the sample and open the bare file, with the agent connected:

```sh
rm -rf /tmp/tally && cp -r examples/tally /tmp/tally
tiza /tmp/tally/src/main.rs
```

Then, to Claude Code (one prompt at a time; wait for each to finish):

1. `Use the tiza MCP server. Call status and get_code: this is main.rs of tally, a small CLI in /tmp/tally/src. Without drawing anything, tell me in three lines what the program does and which files it uses.`
2. `Go to step 1 (next_step) so I see it being drawn. On main.rs at step 1: add_mark kind flow on args::parse at line 10, label "1 parse the flags", color blue, id m1; the same on count::count at line 21, "2 count each file", green, id m2; and on report::table at line 26, "3 print the table", purple, id m3. Then add_note on line 14: "a bad flag ends here: exit code 2" (id n1), and add_box over lines 19-24 labelled "a file it cannot read is skipped" (id b1). Anchor with find, never with columns.`
3. `Now show me args.rs: add_move at step 2 with action code.link, file "args.rs" (relative to main.rs's folder), lines "14-30", then next_step. At step 2: add_box lines 17-25 "every argument is a path, except the flag" in blue (id b2); add_mark kind error on Err at line 21, "main prints it, exits 2" (id k1); add_note on line 26 "no paths at all is an error too" (id n2).`
4. `Next, count.rs: add_move at step 3, code.link to "count.rs" (the whole file), then next_step. At step 3: add_callout lines 11-14, title "bytes come free", text "text.len() is the byte length of the String, so only lines and words need the loop.", cyan (id c1); add_box lines 15-18 "one pass over the lines", green (id b3); highlight split_whitespace() on line 17 with label "runs of spaces count once" (id h1).`
5. `Then report.rs: add_move at step 4, code.link to "report.rs", then next_step. At step 4: add_mark kind new on rows.to_vec() at line 7, "a copy: the caller's rows are untouched" (id k2); add_box lines 8-11 "--by-words: most words first", purple (id b4); add_note on line 14 "right-aligned columns, like wc" (id n3).`
6. `Back to main.rs for the recap: add_move at step 5, code.link to "main.rs" with lines "9-28", and a second move at step 5, action select, from {find: "report::table"} to {find: "opts.sort)"}; then next_step. At step 5: add_callout lines 18-26, placement auto, title "the whole flow", text "args.rs turns the flags into Options, count.rs reads each file once, report.rs sorts and prints. main only wires them together.", cyan (id c2); add_spotlight at step 5 with ids m1, m2, m3 and c2 (id s1).`
7. `Name the steps. There is no MCP tool for that, so run this in the shell: tiza scene patch '{"steps":[{"step":1,"id":"main","title":"main.rs: parse, count, report"},{"step":2,"id":"args","title":"args.rs: one flag, by hand"},{"step":3,"id":"count","title":"count.rs: one pass"},{"step":4,"id":"report","title":"report.rs: sort, then print"},{"step":5,"id":"recap","title":"Back in main.rs"}]}'`
8. `Call check with width 130 and fix anything it reports. Then goto_step "count" to show me it works, and set_step 0.`

Now present it: `P` for presentation mode and `n` through the steps (or `tiza next` / `tiza goto recap` from another terminal). Each `add_*` reply says whether the annotation is on screen (`m1 added at step 1; visible now …`); if the agent ever answers `not visible now`, it says why, and the take is still right. The agent may phrase things differently or pick other columns; `examples/tally-tour.tiza` is the reference result.

The agent's version is already on disk: every call above was autosaved to `/tmp/tally/src/main.rs.tiza`. Check it and render it like any `.tiza` (the file is the take's result; since the [live export change](#planned-changes-that-touch-these-scenarios) a render of the live instance draws the same):

```sh
tiza check /tmp/tally/src/main.rs.tiza --width 130
tiza render /tmp/tally/src/main.rs.tiza --out tally-tour.gif --width 130 --interval 3
tiza render /tmp/tally/src/main.rs.tiza --all-steps --width 130 --out tally-tour-steps/
```

## 2. One agent explains a change to another for review

**For:** a reviewing agent, and the person watching two agents talk through code — a review, or knowledge passing between a coding agent and a checking one.

**Setup:** `examples/tally/` copied to a working folder (`rm -rf /tmp/tally && cp -r examples/tally /tmp/tally`), Tiza on `tiza /tmp/tally/src/args.rs`. Two agent sessions (two Claude Code terminals) with the same `tiza` MCP server registered: each starts its own `tiza mcp`, and both talk to the one instance (the socket serves several clients at once). Optionally `:supervise on`, so every change from either agent waits for your `y`/`n`.

**Script:** annotations carry no author, so each agent signs its own: an id prefix (`a-`, `rev-`) for the tools, and the role at the start of every label and note for the audience (`author: …`, `review: …`).
- To agent A (the author): `You want to add a --by-lines flag to tally. Explain the change on Tiza instead of editing the file: at step 1 box the current flag handling; at step 2 add the new branch as a code.patch move (op insert after the line with "sort = Sort::Words;", typing "line"), and annotate the new lines at step 2. Prefix every id with "a-" and start every label and note with "author:". Leave the view on step 2.`
- To agent B (the reviewer): `Read tiza://scene and tiza://render.md (MCP resources; the presenter is on step 2), then list_annotations. Review the change shown at step 2. Put your findings at step 3 as marks of kind question or warn, or notes, with ids prefixed "rev-" and every label and note starting with "review:". Do not touch annotations you did not write.`
- To agent A again: `list_annotations: answer each rev- annotation at step 4 with a note of your own on the same line, starting with "author:", or fix your move (delete_move, add_move) and say so.`

**What the audience sees:** the flag loop boxed; at step 2 the new branch typing itself in, with the author's notes; at step 3 the reviewer's questions and warnings appear on the same lines, each one signed `review:`; at step 4 the answers, signed `author:`. `p` walks it back, and the typed code goes away again.

**Features:** several MCP clients on one instance, `code.patch` with `typing`, the `question` and `warn` kinds, the `tiza://` resources, `list_annotations`, `delete_move`, `:supervise`, autosave to the sidecar.

**Recording:** three panes (agent A, agent B, Tiza), 3-5 min; afterwards a GIF of the finished review from the sidecar it left: `tiza render /tmp/tally/src/args.rs.tiza --out review.gif`.

## 3. A walkthrough one agent leaves for the next

**For:** the next agent (or teammate) who opens this code tomorrow without the conversation that explained it.

**Setup:** the walkthrough a tour left behind, in a shell variable: `W=/tmp/tally/src/main.rs.tiza`, the sidecar scenario 1 autosaved, or the finished take, `rm -rf /tmp/tally && cp -r examples/tally /tmp/tally && cp examples/tally-tour.tiza /tmp/` and `W=/tmp/tally-tour.tiza` (it names `tally/src/main.rs` relative to itself, so it stays next to the `tally` folder).

**Script:**
- Agent A, after a tour like scenario 1, has nothing to export: the TUI autosaved every step to `/tmp/tally/src/main.rs.tiza`. Hand over the folder (the sidecar names `args.rs`, `count.rs` and `report.rs` by relative path). If the code will change before B reads it, run `:embed` in the TUI first: the sidecar then stores every linked file under `views`.
- Agent B, in a new session and with no TUI: `tiza check "$W"`, then `tiza render "$W" --all-steps --format md --out walkthrough-md/` and read `step-01.md` … in order. Or open it in an instance of its own (`tiza "$W" --session b`, with `tiza mcp --session b`) and read `tiza://render.md` after each `goto_step`.
- To agent B: `Read the walkthrough step by step and answer: where would a --by-lines flag go, and which files change?`

**What the audience sees:** mostly the second agent's answer, quoting frames of a tour it never watched; as a video, the Markdown frames scrolling in a terminal or an editor.

**Features:** the autosaved sidecar of a `code.link` tour, `:embed` (`views`), `tiza check`, `render --all-steps --format md`, `--session`, `goto_step`, the `tiza://` resources.

**Recording:** better as a written example (the frames are text) than as a video; under a minute if recorded.

## 4. Walking through a change, before and after

**For:** a reviewer reading a pull request, or a viewer watching a refactor.

**Setup:** `examples/ownership-moves.rs.tiza`, where step 2 types a borrow and a `len` helper into the code and step 4 replaces the call with `t.len()` and deletes the helper (it embeds its code, so `cp examples/ownership-moves.rs.tiza /tmp/` and `tiza /tmp/ownership-moves.rs.tiza` is enough). For your own change, open the old version of the file from a working folder (`mkdir -p /tmp/review && git show HEAD~1:src/lib.rs > /tmp/review/lib.rs`, then `tiza /tmp/review/lib.rs`) and keep `git diff HEAD~1 -- src/lib.rs` at hand for the agent.

**Script:** Tiza does not read diffs; the agent writes the moves from one. `Run git diff HEAD~1 -- src/lib.rs and read it. Write each hunk as an add_move code.patch at its own step (op insert, replace or delete, typing "line"), oldest hunk first, targeting lines by the visible text around them (after, find) rather than by number; annotate the new code at the step where it exists; if an annotation goes (unanchored) after a replace, give the move a rebind. Then call check and walk the steps with goto_step to confirm the last one matches the new file.`

**What the audience sees:** the old code, then each change typing itself in with a note on why; going back (`p`) undoes it line for line.

**Features:** `code.patch` (`after`, `find`, `occurrence`), `typing` and `speed_ms`, `rebind`, `(unanchored)` in `list_moves`, `check` on move targets.

**Recording:** the native GIF keeps the typed frames (`tiza render examples/ownership-moves.rs.tiza --out moves.gif`); `--out moves.tape` and `vhs` for a terminal-window look; 1-2 min.

## 5. Tracing a bug across files to its fix

**For:** a viewer who likes debugging stories; a teammate who hit the bug.

**Setup:** `examples/tally/` copied to `/tmp/tally`. A real limitation of tally: a file whose name starts with `--` cannot be counted (`tally --notes.txt` says `unknown flag --notes.txt`), because `args.rs` treats any `--` prefix as a flag and there is no `--` separator. `tiza /tmp/tally/src/main.rs`.

**Script:** one file on screen per step, each explained while it is shown; the label on the symptom says where the next step goes. `A user reports: tally --notes.txt says "unknown flag". Trace it on Tiza from the symptom to the cause, one file per step: step 1, box the eprintln in main.rs that prints it, labelled "the message comes from args::parse (next step)"; step 2, link args.rs (code.link) and mark the line that decides it; step 3, spotlight only that branch; step 4, propose the fix as a code.patch typed into args.rs (accept a lone "--" that ends the flags) and annotate it.`

**What the audience sees:** the error message boxed in `main.rs` with a label that names the next file, the step to `args.rs` with an `error` mark on `starts_with("--")`, the rest of the file dimmed, then the fix typing itself in.

**Features:** `code.link`, `error` marks, spotlight, `code.patch` on a linked file, `check`.

**Recording:** 2-3 min live; the GIF of the finished take for a post, rendered from `/tmp/tally/src/main.rs.tiza`.

## 6. Explaining a config file with demo values

**For:** a viewer, or a teammate setting up the project.

**Setup:** examples ship no secrets on purpose: what is in the file is what the audience sees. Make a throwaway file with fake values, never a copy of a real one: `mkdir -p /tmp/config && printf 'DATABASE_URL=postgres://app:demo-pass@db.local/app\nAPI_TOKEN=tk_demo_token_0000\nLOG_LEVEL=info\n' > /tmp/config/demo.env`.

**Script:**
- You: `tiza /tmp/config/demo.env`.
- To the agent: `Call get_code. Explain each setting at step 1 with a note on its line; keep the notes short.`
- To the agent: `Run check and fix what it reports. Then set_step 1.`

**What the audience sees:** the config file with a note per line.

**Features:** `get_code`, `add_note`, the sidecar written next to the file (`/tmp/config/demo.env.tiza`), `tiza check`.

**Recording:** presentation mode; before recording, read the screen: if a real value slipped in, change the file, not the take. Under a minute.

## 7. Live coding: the agent annotates while you edit

**For:** a viewer watching code being written, with the explanation keeping up.

**Setup:** a copy of `examples/tally/` (`rm -rf /tmp/tally && cp -r examples/tally /tmp/tally`), open in the editor and in `tiza /tmp/tally/src/args.rs --follow`. Zed integration: `docs/zed.md`.

**Script:** you add a `--by-lines` flag in the editor and save after each piece; after each save, to the agent: `list_annotations: re-add what the reload dropped, and mark the lines I just added.` Or ask it once to `set_follow` on and keep the annotations up to date.

**What the audience sees:** the editor and Tiza side by side; annotations follow their text as lines move, the ones whose text is gone disappear, and the agent puts new ones on the new code.

**Features:** follow mode and re-anchoring, `set_follow`, `follow.status` (`reloads`, `last_reload`), `list_annotations`, `u` to undo a reload.

**Recording:** a screen recording, 3-5 min; the editor at 50 % and Tiza at 50 % ([Check before you record](../guides/check-before-you-record.md)).

## 8. Supervised session: the agent proposes, you approve on camera

**For:** a viewer who wants to see the human stay in control of what an agent draws.

**Setup:** `examples/src/stack.rs`, bare, from a working folder: `mkdir -p /tmp/stack && cp examples/src/stack.rs /tmp/stack/`, then `tiza /tmp/stack/stack.rs` and `:supervise on`. (`examples/stack-impl.tiza` is a finished take of the same lesson.)

**Script:** `Explain why push and pop take &mut self and peek takes &self: one mark per method, a note on Option<&T>.` Each call waits. Type `:review`, then press `y` for one change, `n` for another and `a` for the rest.

**What the audience sees:** `⏸ N to review · :review` on the bar, the review panel with one row for each change (`▸ p1  add note L4 «…»`), the accepted changes on the code, and the agent saying what it wanted after a rejection instead of retrying.

**Features:** `:supervise`, `:review` and its panel, `rejected`, the presentation-mode chip, `status.pending`.

**Recording:** normal mode (the review panel is the point), 1-2 min; only what you accepted reaches `/tmp/stack/stack.rs.tiza`.

## 9. One concept, step by step

**For:** someone learning one idea — the use Tiza was born for (Rust ownership, in tutorials).

**Setup:** `examples/ownership.rs`, copied alone so it opens bare (`mkdir -p /tmp/lesson && cp examples/ownership.rs /tmp/lesson/`, then `tiza /tmp/lesson/ownership.rs`); the finished lesson is `examples/ownership.rs.tiza` (7 steps), the sidecar of the same file.

**Script:** `Explain move, shared borrow and &mut on this file in seven steps, one colour per value: arrows solid for a move, dashed for a shared borrow, thick for &mut; a callout that sums up at the end. Use a style borrow_mut with define_style.`

**What the audience sees:** the README's ownership take: `s` moves, `t` is borrowed, `u` is borrowed mutably, `takes` drops `s`.

**Features:** marks and kinds, `define_style`, arrows with `stroke`/`heads`, callouts, spotlight, stable geometry across steps, legibility after compression.

**Recording:** presentation mode at 108 columns or more (`tiza check` says so), or the native GIF the README uses; a 5-10 min tutorial segment.

## 10. Same shape, other end: two implementations

**For:** a learner comparing two designs, one after the other.

**Setup:** `examples/stack-and-queue.tiza` over `examples/src/stack.rs` and `examples/src/queue.rs`. To replay it, copy the three together (`mkdir -p /tmp/sq/src && cp examples/stack-and-queue.tiza /tmp/sq/ && cp examples/src/stack.rs examples/src/queue.rs /tmp/sq/src/`) and open `tiza /tmp/sq/stack-and-queue.tiza`.

**Script:** one file per step, the comparison carried by the notes. `Walk me through the stack and the queue, one file at a time: box push/pop in stack.rs; link queue.rs at the next step and box enqueue/dequeue, with a note that says which end each one uses; select push_back to pop_front and pause; then link back to stack.rs and add a note on peek that says what it has in common with queue.rs's peek; last, link queue.rs again with only the lines of enqueue and dequeue.`

**What the audience sees:** each file in its own steps with its own boxes and notes; going back to `stack.rs` brings its annotations back as they were, plus the comparing note (`same signature in both files: a shared borrow`); at the end, `queue.rs` again in a narrower window (lines 17-23), its box still drawn.

**Features:** `code.link` back and forth, `lines` windows, `select`, `cursor`, `wait`.

**Recording:** the native GIF honours the `wait` (`tiza render examples/stack-and-queue.tiza --out sq.gif`); under a minute.

## 11. Answering a question with an annotated frame

**For:** whoever asked in an issue, a pull request, a chat or a doc — no video at all.

**Setup:** any file, for example `examples/tally/src/count.rs` from the working copy (`rm -rf /tmp/tally && cp -r examples/tally /tmp/tally`, then `tiza /tmp/tally/src/count.rs`).

**Script:** `Someone asks why tally counts "a   b" as two words. Answer with one annotated frame: highlight split_whitespace() and add a note, then export_image to /tmp/answer.md (and /tmp/answer.png for the issue).`

**What the audience sees:** a fenced text block (or a PNG) with the line highlighted and the note beside it, pasted under the question.

**Features:** `export_image` to `.md`, `.txt` or `.png`, `render --all-steps` for a short series.

**Recording:** a still, not a video; `tiza render /tmp/tally/src/count.rs.tiza --out answer.md --step 1` does the same from the shell.

## 12. Hands-free: a looping screen or an unattended take

**For:** passers-by at a booth or a meetup screen; a recording with no one pressing keys.

**Setup:** a copy of the finished tour: `rm -rf /tmp/tally && cp -r examples/tally /tmp/tally && cp examples/tally-tour.tiza /tmp/`, opened with `tiza /tmp/tally-tour.tiza --present`.

**Script:** from another terminal, give every step a duration, step 0 included, and loop it: `tiza scene patch '{"steps":[{"step":0,"id":"intro","auto_advance_ms":2000},{"step":1,"id":"main","title":"main.rs: parse, count, report","auto_advance_ms":4000},{"step":2,"id":"args","title":"args.rs: one flag, by hand","auto_advance_ms":4000},{"step":3,"id":"count","title":"count.rs: one pass","auto_advance_ms":4000},{"step":4,"id":"report","title":"report.rs: sort, then print","auto_advance_ms":4000},{"step":5,"id":"recap","title":"Back in main.rs","auto_advance_ms":6000}],"loop":true}'` (the list replaces the old one). Step 0 needs its entry too: without an `auto_advance_ms` there, the tour does not start by itself and stops when the loop comes back to it. The copy keeps the durations, so the next start loops on its own. For an editor-driven take instead, bind `tiza next`, `tiza prev` and `tiza goto recap` to keys (`docs/zed.md`).

**What the audience sees:** the tour playing by itself: step 0 for 2 s, steps 1-4 for 4 s each, the recap for 6 s, then back to step 0 and round again, forever (24 s a loop).

**Features:** `auto_advance_ms`, `loop`, presentation mode over a `code.link` tour, `tiza next|prev|goto`; the native GIF and MP4 hold such a step for exactly its `auto_advance_ms`.

**Recording:** none needed for the booth. For a video, `tiza render /tmp/tally-tour.tiza --out loop.gif` holds each step exactly its `auto_advance_ms`.

## Planned changes that touch these scenarios

Nothing above waits on these; they change how a script can be written once they land.

- **Exporting the live instance of a multi-file document** (1, 3, 5, 10, 12). Landed, waiting for Luis's QA: `tiza render` with no FILE (or `--socket`) and the MCP `export_image` now draw the files a `code.link` move shows, as the instance has them on screen, so a script can export the live instance instead of the sidecar. The scripts keep rendering the `.tiza` the TUI autosaved, which is the take's result either way.
