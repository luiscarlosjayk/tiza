# Demo scenarios

This page gives ideas for demos in which Tiza and an agent explain code to a person or to another agent. Each scenario gives a sample under `examples/`, a script, what the audience sees and how to record it.

Tiza is a terminal tool that draws annotations on code. To install it, see [Install Tiza](../guides/install.md). The agent is Claude Code, Codex or another MCP client.

| Scenario | For | Sample |
|---|---|---|
| [1. A codebase tour across a few files](#1-a-codebase-tour-across-a-few-files) | A viewer or a new teammate | `examples/tally/` |
| [2. A review between two agents](#2-a-review-between-two-agents) | A reviewer agent and the people who watch it | `examples/tally/` |
| [3. A walkthrough for the next agent](#3-a-walkthrough-for-the-next-agent) | The next agent or teammate | The document of scenario 1 |
| [4. A change, before and after](#4-a-change-before-and-after) | A reviewer or a viewer | `examples/ownership-moves.rs.tiza` |
| [5. A bug, from the symptom to the fix](#5-a-bug-from-the-symptom-to-the-fix) | A viewer or a teammate | `examples/tally/` |
| [6. A `.env` file with demo values](#6-a-env-file-with-demo-values) | A viewer or a teammate | A demo `.env` file |
| [7. Live coding with follow mode](#7-live-coding-with-follow-mode) | A viewer | `examples/tally/` |
| [8. A supervised session](#8-a-supervised-session) | A viewer | `examples/src/stack.rs` |
| [9. One concept, step by step](#9-one-concept-step-by-step) | A learner | `examples/ownership.rs` |
| [10. Two implementations, one after the other](#10-two-implementations-one-after-the-other) | A learner | `examples/stack-and-queue.tiza` |
| [11. An answer as an annotated frame](#11-an-answer-as-an-annotated-frame) | The person who asked a question | `examples/tally/src/count.rs` |
| [12. A looping screen or an unattended recording](#12-a-looping-screen-or-an-unattended-recording) | People at a booth, or a viewer | `examples/tally-tour.tiza` |

## Before you start

- Register the MCP server one time. See [Connect an agent](../agents/setup.md). Then run Tiza in one terminal and the agent in another terminal.
- Run the commands from a folder that holds `examples/`: a clone of the repository or the folder of the release archive.
- Work on a copy of the sample, not on the files in `examples/`. Remove the old copy before you copy again:

  ```sh
  rm -rf /tmp/tally && cp -r examples/tally /tmp/tally
  ```

- When `/tmp/tally` exists, `cp -r` copies into `/tmp/tally/tally`. Then Tiza opens the old file and its document from an earlier scenario.
- Tiza saves each change to the document next to the file, also each change of the agent. For example, `tiza /tmp/tally/src/main.rs` saves to `/tmp/tally/src/main.rs.tiza`. See [Saving](../concepts/saving.md).
- That document is the result. Check it, export it or give it to another agent.
- The document reads each copied file by a relative path. Move the whole folder with it, or type `:embed` to store the code of all its files in the document.
- Before each screen recording or export, run `tiza check FILE` on the document, or ask the agent to call `check`. Fix each finding that is not on purpose. The warnings of `examples/ownership-moves.rs.tiza` and `examples/stack-and-queue.tiza` are on purpose. See [Check before you record](../guides/check-before-you-record.md).

## 1. A codebase tour across a few files

- **Sample:** Copy `examples/tally/`, a Rust CLI of four files that counts lines, words and bytes. Run `tiza /tmp/tally/src/main.rs` in a terminal 130 columns wide. In a narrower terminal, Tiza can put the labels under their lines. See [Narrow terminals](../guides/check-before-you-record.md#narrow-terminals).
- **Script:** Ask the agent to show the program one file at each step. The agent shows each file with an `add_move` of action `code.link`, and annotates it at that step. A label can tell which file comes next. [A tour of tally](tour-of-tally.md) gives the exact prompts.
- **The audience sees:** Step 1 shows the three calls of `main.rs` as a numbered flow. Steps 2 to 4 show `args.rs`, `count.rs` and `report.rs`, each with its own annotations. Step 5 shows `main.rs` again, with a selection, a summary callout and a spotlight.
- **Recording:** Make a screen recording of Tiza and the agent side by side, 2 to 4 minutes. Or play the finished tour, `examples/tally-tour.tiza`, as [scenario 12](#12-a-looping-screen-or-an-unattended-recording) does. Press `n` for each step. For a GIF, run `tiza render examples/tally-tour.tiza --out tally-tour.gif --width 130 --interval 3`.

## 2. A review between two agents

- **Sample:** Copy `examples/tally/` and run `tiza /tmp/tally/src/args.rs`. Start two agent sessions with the same `tiza` MCP server. Each session starts its own `tiza mcp`, and both talk to the same instance. To accept or reject each change yourself, type `:supervise on`.
- **Script:** An annotation has no author field. Thus each agent signs its annotations with an id prefix and with a role at the start of each label and note.
  1. To agent A, the author: `Explain a new --by-lines flag on Tiza; do not edit the file. At step 1, box the flag handling. At step 2, add a code.patch move (op insert after "sort = Sort::Words;", typing "line") and annotate the new lines. Prefix every id with "a-" and every label and note with "author:". Then call set_step 2.`
  2. To agent B, the reviewer: `Read tiza://scene and tiza://render.md, then call list_annotations. Review the change at step 2. At step 3, add marks of kind question or warn, or notes. Prefix every id with "rev-" and every label and note with "review:". Do not change annotations that you did not write.`
  3. To agent A again: `Call list_annotations. At step 4, answer each rev- annotation with a note on the same line that starts with "author:", or fix your move with delete_move and add_move.`
- **The audience sees:** The flag loop in a box. At step 2, Tiza types the new branch, with the notes of the author. At step 3, the questions and warnings of the reviewer show on the same lines. At step 4, the answers show. Press `p` to go back. The typed code goes away.
- **Recording:** Show three terminals side by side: agent A, agent B and Tiza. The screen recording takes 3 to 5 minutes. Then export a GIF of the review from the document: `tiza render /tmp/tally/src/args.rs.tiza --out review.gif`.

## 3. A walkthrough for the next agent

- **Sample:** Use the document of scenario 1, `/tmp/tally/src/main.rs.tiza`. Or copy the finished tour next to a copy of `tally`: `rm -rf /tmp/tally && cp -r examples/tally /tmp/tally && cp examples/tally-tour.tiza /tmp/`. The tour names `tally/src/main.rs` relative to its own folder, so keep it next to the `tally` folder.
- **Script:**
  1. Agent A has nothing to export, because Tiza saved each change. Give the whole folder to agent B. If the code changes before B reads it, type `:embed` first.
  2. Agent B needs no instance. It runs `tiza check FILE`, then `tiza render FILE --all-steps --format md --out walkthrough-md/`. Then it reads `step-01.md` and the next files in order.
  3. Or agent B opens the document in its own instance: `tiza FILE --session b`, with its MCP server started as `tiza mcp --session b`. It reads `tiza://render.md` after each `goto_step`.
  4. To agent B: `Read the walkthrough step by step. Where would a --by-lines flag go, and which files change?`
- **The audience sees:** The answer of agent B, which quotes frames of a tour that it did not watch. In a video, the Markdown frames scroll in a terminal or an editor.
- **Recording:** A written example works better than a video, because the frames are text. A screen recording takes less than one minute.

## 4. A change, before and after

- **Sample:** In `examples/ownership-moves.rs.tiza`, step 2 types a borrow and a `len` helper into the code. Step 4 replaces the call with `t.len()` and deletes the helper. The document holds its code, so a copy is enough: `cp examples/ownership-moves.rs.tiza /tmp/`, then run `tiza /tmp/ownership-moves.rs.tiza`.
  - For your own change, open the old version of the file from a working folder: `mkdir -p /tmp/review && git show HEAD~1:src/lib.rs > /tmp/review/lib.rs`, then run `tiza /tmp/review/lib.rs`.
- **Script:** Tiza does not read a diff, so the agent writes the moves from it. Send this prompt:

  ```text
  Run git diff HEAD~1 -- src/lib.rs. Write each hunk as an add_move code.patch at its own step, with typing "line". Find each target by its text (after, find), not by its line number. Annotate the new code at its step. Give a rebind to a move that leaves an annotation (unanchored). Then call check, and walk the steps with goto_step.
  ```

- **The audience sees:** The old code, then each change as Tiza types it, with a note on why. Press `p` to go back to the code before the change.
- **Recording:** The GIF keeps the typed frames: `tiza render examples/ownership-moves.rs.tiza --out moves.gif`. For the look of a terminal window, export `--out moves.tape` and run `vhs`. A screen recording takes 1 to 2 minutes.

## 5. A bug, from the symptom to the fix

- **Sample:** Copy `examples/tally/` and run `tiza /tmp/tally/src/main.rs`. tally cannot count a file whose name starts with `--`, because `args.rs` reads each argument that starts with `--` as a flag. With Rust installed, `cargo run -- --notes.txt` in `/tmp/tally` prints `tally: unknown flag --notes.txt`.
- **Script:** Send these prompts to the agent, one at a time:
  1. `A user reports: tally --notes.txt says "unknown flag". Trace it on Tiza from the symptom to the cause, one file at each step.`
  2. `At step 1, box the eprintln in main.rs that prints it, with the label "the message comes from args::parse (next step)".`
  3. `At step 2, show args.rs with a code.link move and mark the line that decides it. At step 3, spotlight only that branch.`
  4. `At step 4, type the fix into args.rs as a code.patch: a lone "--" ends the flags. Annotate the fix.`
- **The audience sees:** The error message in a box in `main.rs`, with a label that names the next file. Then `args.rs`, with a mark on `starts_with("--")` and the rest of the file dimmed. Then Tiza types the fix.
- **Recording:** A live screen recording of 2 to 3 minutes. For a post, run `tiza render /tmp/tally/src/main.rs.tiza --out bug.gif`.

## 6. A `.env` file with demo values

- **Sample:** Make a demo file with fake values, never a copy of a real file. The audience sees all the text of the file. See [Check before you record](../guides/check-before-you-record.md#checklist):

  ```sh
  mkdir -p /tmp/config && printf 'DATABASE_URL=postgres://app:demo-pass@db.local/app\nAPI_TOKEN=demo-token\nLOG_LEVEL=info\n' > /tmp/config/demo.env
  ```

- **Script:** Run `tiza /tmp/config/demo.env`. Then ask the agent: `Call get_code. Explain each setting at step 1 with a short note on its line.` Then: `Run check and fix each finding. Then call set_step 1.`
- **The audience sees:** The `.env` file with a note on each line. Tiza saves the notes to `/tmp/config/demo.env.tiza`.
- **Recording:** Use presentation mode. The screen recording takes less than one minute. Before you record, read the screen. If a real value is on it, change the file and record again.

## 7. Live coding with follow mode

- **Sample:** Copy `examples/tally/`. Open `/tmp/tally/src/args.rs` in your editor. In another terminal, run `tiza /tmp/tally/src/args.rs --follow`. For Zed, see [Tiza from Zed](../zed.md).
- **Script:** Add a `--by-lines` flag in the editor, and save after each part. After each save, tell the agent: `Call list_annotations, add again what the reload dropped, and mark the lines that I just added.` If you started Tiza without `--follow`, the agent can turn on follow mode with `set_follow`. See [Agents and follow mode](../guides/follow-the-editor.md#agents-and-follow-mode).
- **The audience sees:** The editor and Tiza side by side. The annotations move with their text. The agent adds annotations on the new code.
- **Recording:** A screen recording of 3 to 5 minutes. Press `u` in Tiza to undo a reload. See [Undo a reload](../guides/follow-the-editor.md#undo-a-reload) and [What happens to annotations](../guides/follow-the-editor.md#what-happens-to-annotations).

## 8. A supervised session

- **Sample:** Copy `examples/src/stack.rs` alone, so that it opens with no annotations: `mkdir -p /tmp/stack && cp examples/src/stack.rs /tmp/stack/`. Run `tiza /tmp/stack/stack.rs` and type `:supervise on`. `examples/stack-impl.tiza` is a finished document of the same lesson.
- **Script:** Ask the agent: `Explain why push and pop take &mut self and peek takes &self: one mark per method, a note on Option<&T>.` Each change of the agent waits 60 s for you, then Tiza rejects it. Type `:review`. Press `y` to accept a change, `n` to reject one and `a` to accept all the changes that are left.
- **The audience sees:** `⏸ N to review · :review` on the status bar, the review panel with one row for each change, and the accepted changes on the code. For each change that you reject, the agent gets the error `rejected`. See [Supervised mode](../agents/supervised-mode.md).
- **Recording:** Use normal mode. The review panel is the subject of this scenario. The screen recording takes 1 to 2 minutes. Only the changes that you accept go to `/tmp/stack/stack.rs.tiza`.

## 9. One concept, step by step

- **Sample:** Copy `examples/ownership.rs` alone, so that it opens with no annotations: `mkdir -p /tmp/lesson && cp examples/ownership.rs /tmp/lesson/`, then `tiza /tmp/lesson/ownership.rs`. The finished lesson is `examples/ownership.rs.tiza`.
- **Script:** Ask the agent: `Explain move, shared borrow and &mut on this file in seven steps, one color per value: arrows solid for a move, dashed for a shared borrow, thick for &mut; a callout that sums up at the end. Use a style borrow_mut with define_style.`
- **The audience sees:** The ownership lesson of the README: `s` moves, `t` is borrowed, `u` is borrowed mutably and `takes` drops `s`.
- **Recording:** A part of a tutorial of 5 to 10 minutes, in presentation mode or as the GIF of the README. See [The width line](../guides/check-before-you-record.md#the-width-line).

## 10. Two implementations, one after the other

- **Sample:** `examples/stack-and-queue.tiza` shows `examples/src/stack.rs` and `examples/src/queue.rs`. Copy the three files together: `mkdir -p /tmp/sq/src && cp examples/stack-and-queue.tiza /tmp/sq/ && cp examples/src/stack.rs examples/src/queue.rs /tmp/sq/src/`. Then run `tiza /tmp/sq/stack-and-queue.tiza`.
- **Script:** Send these prompts to the agent, one at a time:
  1. `Walk me through the stack and the queue, one file at each step. Box push and pop in stack.rs. Then link queue.rs, box enqueue and dequeue, and note which end each one uses.`
  2. `Select push_back to pop_front, then pause. Link back to stack.rs and note what peek has in common with the peek of queue.rs. Last, link queue.rs with only the lines of enqueue and dequeue.`
- **The audience sees:** Each file at its own steps, with its own annotations. When the tour goes back to `stack.rs`, its annotations come back as they were, with the new note. At the end, `queue.rs` shows in a smaller window, lines 17 to 23, and its box is still drawn.
- **Recording:** The GIF shows the pause of the `wait` move: `tiza render examples/stack-and-queue.tiza --out sq.gif`. The GIF takes less than one minute.

## 11. An answer as an annotated frame

- **Sample:** Any file. For example, copy `examples/tally/` and run `tiza /tmp/tally/src/count.rs`.
- **Script:** Ask the agent: `Someone asks why tally counts "a   b" as two words. Answer with one annotated frame: highlight split_whitespace() and add a note. Then call export_image to /tmp/answer.md, and to /tmp/answer.png for the issue.`
- **The audience sees:** A Markdown code block or a PNG under the question, with the line highlighted and the note next to it.
- **Recording:** None. The result is a still, not a video. From the shell, `tiza render /tmp/tally/src/count.rs.tiza --out answer.md --step 1` does the same. For a short series, run `tiza render /tmp/tally/src/count.rs.tiza --all-steps --format md --out answer-steps/`. See [Exports](../reference/exports.md).

## 12. A looping screen or an unattended recording

- **Sample:** Copy the finished tour as in [scenario 3](#3-a-walkthrough-for-the-next-agent). Run `tiza /tmp/tally-tour.tiza --present`.
- **Script:** From another terminal, give each step a time, and turn on the loop. The new list replaces the old list. Give step 0 a time too. Without it, the tour does not start by itself, and it stops when the loop comes back to step 0.

  ```sh
  tiza scene patch '{"steps":[{"step":0,"id":"intro","auto_advance_ms":2000},{"step":1,"id":"main","title":"main.rs: parse, count, report","auto_advance_ms":4000},{"step":2,"id":"args","title":"args.rs: one flag, by hand","auto_advance_ms":4000},{"step":3,"id":"count","title":"count.rs: one pass","auto_advance_ms":4000},{"step":4,"id":"report","title":"report.rs: sort, then print","auto_advance_ms":4000},{"step":5,"id":"recap","title":"Back in main.rs","auto_advance_ms":6000}],"loop":true}'
  ```

- **The audience sees:** The tour plays by itself: step 0 for 2 s, steps 1 to 4 for 4 s each and the recap for 6 s. Then it starts again at step 0, without end. One loop takes 24 s.
- **Recording:** The booth needs no recording. Tiza saves the times in the copy, so the tour also plays by itself when you open it again. For a recording, `tiza render /tmp/tally-tour.tiza --out loop.gif` holds each step for its `auto_advance_ms`.
  - To step the tour from your editor, bind `tiza next`, `tiza prev` and `tiza goto recap` to keys. See [Tiza from Zed](../zed.md) and [Named steps, auto-advance and loop](../agents/moves-and-steps.md#named-steps-auto-advance-and-loop).
