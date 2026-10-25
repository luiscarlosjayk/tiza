# A tour of tally

In this tutorial, an agent draws a tour of a small program, one file at each step. You get a document that you can present and export.

Tiza is a terminal tool that draws annotations on code. To install it, see [Install Tiza](../guides/install.md). This tour is scenario 1 of [Demo scenarios](scenarios.md).

## What you need

- Tiza and an agent with the Tiza MCP server. See [Connect an agent](../agents/setup.md).
- A folder that holds `examples/`: a clone of the repository or the folder of the release archive. Run the commands from that folder.
- A terminal 130 columns wide. In a narrower terminal, Tiza can put the labels under their lines. See [Narrow terminals](../guides/check-before-you-record.md#narrow-terminals).

## Steps

1. Copy the sample and open its entry point:

   ```sh
   rm -rf /tmp/tally && cp -r examples/tally /tmp/tally
   tiza /tmp/tally/src/main.rs
   ```

2. Give the prompts of the next steps to the agent, for example Claude Code. Send one prompt at a time. Wait until the agent is done. The numbers in the prompts are the steps of the document, not the numbers of this list.
3. Ask the agent to read the code:

   ```text
   Use the tiza MCP server. Call status and get_code: this is main.rs of tally, a small CLI in /tmp/tally/src. Without drawing anything, tell me in three lines what the program does and which files it uses.
   ```

4. Draw on `main.rs` in step 1 of the document:

   ```text
   Go to step 1 (next_step) so I see it being drawn. On main.rs at step 1: add_mark kind flow on args::parse at line 10, label "1 parse the flags", color blue, id m1; the same on count::count at line 21, "2 count each file", green, id m2; and on report::table at line 26, "3 print the table", purple, id m3. Then add_note on line 14: "a bad flag ends here: exit code 2" (id n1), and add_box over lines 19-24 labelled "a file it cannot read is skipped" (id b1). Anchor with find, never with columns.
   ```

5. Show `args.rs` in step 2 of the document:

   ```text
   Now show me args.rs: add_move at step 2 with action code.link, file "args.rs" (relative to main.rs's folder), lines "14-30", then next_step. At step 2: add_box lines 17-25 "every argument is a path, except the flag" in blue (id b2); add_mark kind error on Err at line 21, "main prints it, exits 2" (id k1); add_note on line 26 "no paths at all is an error too" (id n2).
   ```

6. Show `count.rs` in step 3 of the document:

   ```text
   Next, count.rs: add_move at step 3, code.link to "count.rs" (the whole file), then next_step. At step 3: add_callout lines 11-14, title "bytes come free", text "text.len() is the byte length of the String, so only lines and words need the loop.", cyan (id c1); add_box lines 15-18 "one pass over the lines", green (id b3); highlight split_whitespace() on line 17 with label "runs of spaces count once" (id h1).
   ```

7. Show `report.rs` in step 4 of the document:

   ```text
   Then report.rs: add_move at step 4, code.link to "report.rs", then next_step. At step 4: add_mark kind new on rows.to_vec() at line 7, "a copy: the caller's rows are untouched" (id k2); add_box lines 8-11 "--by-words: most words first", purple (id b4); add_note on line 14 "right-aligned columns, like wc" (id n3).
   ```

8. Go back to `main.rs` for the summary in step 5 of the document:

   ```text
   Back to main.rs for the recap: add_move at step 5, code.link to "main.rs" with lines "9-28", and a second move at step 5, action select, from {find: "report::table"} to {find: "opts.sort)"}; then next_step. At step 5: add_callout lines 18-26, placement auto, title "the whole flow", text "args.rs turns the flags into Options, count.rs reads each file once, report.rs sorts and prints. main only wires them together.", cyan (id c2); add_spotlight at step 5 with ids m1, m2, m3 and c2 (id s1).
   ```

9. Name the steps. No MCP tool sets the names, so the agent runs a shell command:

   ```text
   Name the steps. There is no MCP tool for that, so run this in the shell: tiza scene patch '{"steps":[{"step":1,"id":"main","title":"main.rs: parse, count, report"},{"step":2,"id":"args","title":"args.rs: one flag, by hand"},{"step":3,"id":"count","title":"count.rs: one pass"},{"step":4,"id":"report","title":"report.rs: sort, then print"},{"step":5,"id":"recap","title":"Back in main.rs"}]}'
   ```

10. Check the tour:

    ```text
    Call check with width 130 and fix each finding. Then goto_step "count" to show me it works, and set_step 0.
    ```

11. Present the tour. Press `P` for presentation mode, then `n` for each step. See [Keys](../reference/keys.md). From another terminal, `tiza next` and `tiza goto recap` also change the step.

The reply of each tool that adds an annotation tells if the annotation is on screen. It starts with `ID added at step N; visible now`, or it says `not visible now` and gives the reason. After you name the steps, the title of the step comes after `step N`. A hidden annotation is not an error. See [Is it on screen?](../agents/draw-for-a-presenter.md#is-it-on-screen).

## Check and export the tour

`examples/tally-tour.tiza` is the result of the prompts on this page. It is a document over `examples/tally/src/main.rs`, a Rust CLI that counts lines, words and bytes.

- Four `code.link` moves show `args.rs` (lines 14 to 30), `count.rs`, `report.rs` and then `main.rs` again (lines 9 to 28).
- A `select` move selects the `report::table` call at the last step.
- The steps have the names `main`, `args`, `count`, `report` and `recap`.
- `tiza check examples/tally-tour.tiza --width 130` reports no findings.

Tiza saved each change of the agent to `/tmp/tally/src/main.rs.tiza`. See [Saving](../concepts/saving.md). That document is your tour. Check it and export it:

```sh
tiza check /tmp/tally/src/main.rs.tiza --width 130
tiza render /tmp/tally/src/main.rs.tiza --out tally-tour.gif --width 130 --interval 3
tiza render /tmp/tally/src/main.rs.tiza --all-steps --width 130 --out tally-tour-steps/
```

- The agent can use other words or other columns. `examples/tally-tour.tiza` is the reference result.
- A `tiza render` without a document exports the live instance. At the same `--width`, it gives the same frames as the saved document while autosave is on. See [The live instance](../reference/exports.md#the-live-instance).

## Where next

- [Demo scenarios](scenarios.md) gives more ideas for recordings with an agent.
- [Change the code between steps](../agents/moves-and-steps.md) tells how `code.link` and the other moves work.
- [Exports](../reference/exports.md) lists the formats and the options of `tiza render`.
