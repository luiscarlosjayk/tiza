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

   Step 1 of the document has three marks, a note and a box on `main.rs`. At 92 columns, the notes, the marks and the callouts go under their lines. In a terminal of 130 columns, most of them go in the right margin.

   <details>
   <summary>Step 1 of the document, as <code>tiza render --width 92</code> draws it</summary>

   <!-- readme-images:frame doc=examples/tally-tour.tiza step=1 width=92 -->
   **Codebase tour: tally, a tiny wc in four files**

   ```text
     1    //! tally: count lines, words and bytes, like a tiny `wc`.
     2
     3    mod args;
     4    mod count;
     5    mod report;
     6
     7    use std::process::ExitCode;
     8
     9    fn main() -> ExitCode {
    10        let opts = match args::parse(std::env::args().skip(1)) {
          · → flow: 1 parse the flags
    11            Ok(opts) => opts,
    12            Err(msg) => {
    13                eprintln!("tally: {msg}");
    14                return ExitCode::from(2);
          ┆ a bad flag ends here: exit code 2
    15            }
    16        };
    17
    18        let mut rows = Vec::new();
       ╭─ a file it cannot read is skipped ──────────────────────────────────────────────────╮
    19 │      for path in &opts.paths {                                                      │
    20 │          match std::fs::read_to_string(path) {                                      │
    21 │              Ok(text) => rows.push((path.clone(), count::count(&text))),            │
       │  · → flow: 2 count each file                                                        │
    22 │              Err(e) => eprintln!("tally: {path}: {e}"),                             │
    23 │          }                                                                          │
    24 │      }                                                                              │
       ╰─────────────────────────────────────────────────────────────────────────────────────╯
    25
    26        print!("{}", report::table(&rows, opts.sort));
          · → flow: 3 print the table
    27        ExitCode::SUCCESS
    28    }
   ```
   <!-- /readme-images:frame -->

   </details>

5. Show `args.rs` in step 2 of the document:

   ```text
   Now show me args.rs: add_move at step 2 with action code.link, file "args.rs" (relative to main.rs's folder), lines "14-30", then next_step. At step 2: add_box lines 17-25 "every argument is a path, except the flag" in blue (id b2); add_mark kind error on Err at line 21, "main prints it, exits 2" (id k1); add_note on line 26 "no paths at all is an error too" (id n2).
   ```

   Step 2 shows lines 14 to 30 of `args.rs`, with a box, a mark and a note.

   <details>
   <summary>Step 2 of the document, as <code>tiza render --width 92</code> draws it</summary>

   <!-- readme-images:frame doc=examples/tally-tour.tiza step=2 width=92 -->
   **Codebase tour: tally, a tiny wc in four files**

   ```text
    14    pub fn parse(args: impl Iterator<Item = String>) -> Result<Options, String> {
    15        let mut paths = Vec::new();
    16        let mut sort = Sort::Name;
       ╭─ every argument is a path, except the flag ─────────────────────────────────────────╮
    17 │      for arg in args {                                                              │
    18 │          if arg == "--by-words" {                                                   │
    19 │              sort = Sort::Words;                                                    │
    20 │          } else if arg.starts_with("--") {                                          │
    21 │              return Err(format!("unknown flag {arg}"));                             │
       │  · ✗ error: main prints it, exits 2                                                 │
    22 │          } else {                                                                   │
    23 │              paths.push(arg);                                                       │
    24 │          }                                                                          │
    25 │      }                                                                              │
       ╰─────────────────────────────────────────────────────────────────────────────────────╯
    26        if paths.is_empty() {
          ┆ no paths at all is an error too
    27            return Err("usage: tally [--by-words] FILE...".into());
    28        }
    29        Ok(Options { paths, sort })
    30    }
   ```
   <!-- /readme-images:frame -->

   </details>

6. Show `count.rs` in step 3 of the document:

   ```text
   Next, count.rs: add_move at step 3, code.link to "count.rs" (the whole file), then next_step. At step 3: add_callout lines 11-14, title "bytes come free", text "text.len() is the byte length of the String, so only lines and words need the loop.", cyan (id c1); add_box lines 15-18 "one pass over the lines", green (id b3); highlight split_whitespace() on line 17 with label "runs of spaces count once" (id h1).
   ```

   Step 3 shows `count.rs`, with a callout, a box and a highlight.

   <details>
   <summary>Step 3 of the document, as <code>tiza render --width 92</code> draws it</summary>

   <!-- readme-images:frame doc=examples/tally-tour.tiza step=3 width=92 -->
   **Codebase tour: tally, a tiny wc in four files**

   ```text
     1    //! One pass over the text: lines, words and bytes.
     2
     3    #[derive(Clone, Copy, Default)]
     4    pub struct Counts {
     5        pub lines: usize,
     6        pub words: usize,
     7        pub bytes: usize,
     8    }
     9
    10    pub fn count(text: &str) -> Counts {
    11        let mut c = Counts {
    12            bytes: text.len(),
    13            ..Counts::default()
    14        };
          ╭─ bytes come free ─────────────────────────────────────────────────────────────╮
          │ text.len() is the byte length of the String, so only lines and words need the │
          │ loop.                                                                         │
          ╰───────────────────────────────────────────────────────────────────────────────╯
       ╭─ one pass over the lines ───────────────────────────────────────────────────────────╮
    15 │      for line in text.lines() {                                                     │
    16 │          c.lines += 1;                                                              │
    17 │          c.words += line.split_whitespace().count();                                │
       │  · runs of spaces count once                                                        │
    18 │      }                                                                              │
       ╰─────────────────────────────────────────────────────────────────────────────────────╯
    19        c
    20    }
   ```
   <!-- /readme-images:frame -->

   </details>

7. Show `report.rs` in step 4 of the document:

   ```text
   Then report.rs: add_move at step 4, code.link to "report.rs", then next_step. At step 4: add_mark kind new on rows.to_vec() at line 7, "a copy: the caller's rows are untouched" (id k2); add_box lines 8-11 "--by-words: most words first", purple (id b4); add_note on line 14 "right-aligned columns, like wc" (id n3).
   ```

   Step 4 shows `report.rs`, with a mark, a box and a note.

   <details>
   <summary>Step 4 of the document, as <code>tiza render --width 92</code> draws it</summary>

   <!-- readme-images:frame doc=examples/tally-tour.tiza step=4 width=92 -->
   **Codebase tour: tally, a tiny wc in four files**

   ```text
     1    //! Turns the counts into the table the user sees.
     2
     3    use crate::args::Sort;
     4    use crate::count::Counts;
     5
     6    pub fn table(rows: &[(String, Counts)], sort: Sort) -> String {
     7        let mut rows = rows.to_vec();
          · + new: a copy: the caller's rows are untouched
       ╭─ --by-words: most words first ──────────────────────────────────────────────────────╮
     8 │      match sort {                                                                   │
     9 │          Sort::Name => rows.sort_by(|a, b| a.0.cmp(&b.0)),                          │
    10 │          Sort::Words => rows.sort_by(|a, b| b.1.words.cmp(&a.1.words)),             │
    11 │      }                                                                              │
       ╰─────────────────────────────────────────────────────────────────────────────────────╯
    12        let mut out = String::new();
    13        for (path, c) in &rows {
    14            out += &format!("{:>6} {:>6} {:>8}  {path}\n", c.lines, c.words, c.bytes);
          ┆ right-aligned columns, like wc
    15        }
    16        out
    17    }
   ```
   <!-- /readme-images:frame -->

   </details>

8. Go back to `main.rs` for the summary in step 5 of the document:

   ```text
   Back to main.rs for the recap: add_move at step 5, code.link to "main.rs" with lines "9-28", and a second move at step 5, action select, from {find: "report::table"} to {find: "opts.sort)"}; then next_step. At step 5: add_callout lines 18-26, placement auto, title "the whole flow", text "args.rs turns the flags into Options, count.rs reads each file once, report.rs sorts and prints. main only wires them together.", cyan (id c2); add_spotlight at step 5 with ids m1, m2, m3 and c2 (id s1).
   ```

   Step 5 shows `main.rs` again, with the callout of the recap. A text frame does not show the spotlight and the selection.

   <details>
   <summary>Step 5 of the document, as <code>tiza render --width 92</code> draws it</summary>

   <!-- readme-images:frame doc=examples/tally-tour.tiza step=5 width=92 -->
   **Codebase tour: tally, a tiny wc in four files**

   ```text
     9    fn main() -> ExitCode {
    10        let opts = match args::parse(std::env::args().skip(1)) {
          · → flow: 1 parse the flags
    11            Ok(opts) => opts,
    12            Err(msg) => {
    13                eprintln!("tally: {msg}");
    14                return ExitCode::from(2);
          ┆ a bad flag ends here: exit code 2
    15            }
    16        };
    17
    18        let mut rows = Vec::new();
       ╭─ a file it cannot read is skipped ──────────────────────────────────────────────────╮
    19 │      for path in &opts.paths {                                                      │
    20 │          match std::fs::read_to_string(path) {                                      │
    21 │              Ok(text) => rows.push((path.clone(), count::count(&text))),            │
       │  · → flow: 2 count each file                                                        │
    22 │              Err(e) => eprintln!("tally: {path}: {e}"),                             │
    23 │          }                                                                          │
    24 │      }                                                                              │
       ╰─────────────────────────────────────────────────────────────────────────────────────╯
    25
    26        print!("{}", report::table(&rows, opts.sort));
          · → flow: 3 print the table
          ╭─ the whole flow ───────────────────────────────────────────────────────────────╮
          │ args.rs turns the flags into Options, count.rs reads each file once, report.rs │
          │ sorts and prints. main only wires them together.                               │
          ╰────────────────────────────────────────────────────────────────────────────────╯
    27        ExitCode::SUCCESS
    28    }
   ```
   <!-- /readme-images:frame -->

   </details>

9. Name the steps. No MCP tool sets the names, so the agent runs a shell command:

   ```text
   Name the steps. There is no MCP tool for that, so run this in the shell: tiza scene patch '{"steps":[{"step":1,"id":"main","title":"main.rs: parse, count, report"},{"step":2,"id":"args","title":"args.rs: one flag, by hand"},{"step":3,"id":"count","title":"count.rs: one pass"},{"step":4,"id":"report","title":"report.rs: sort, then print"},{"step":5,"id":"recap","title":"Back in main.rs"}]}'
   ```

10. Check the tour:

    ```text
    Call check with width 130 and fix each finding. Then goto_step "count" to show me it works, and set_step 0.
    ```

11. Present the tour. Press `P` for presentation mode, then `n` for each step. See [Keys](../reference/keys.md). From another terminal, `tiza next` and `tiza goto recap` also change the step.

The reply of each tool that adds an annotation tells if the annotation is on screen. It starts with `ID added at step N; visible now`, or it says `not visible now` and gives the reason. After you name the steps, the title of the step comes after `step N`. A hidden annotation is not an error. See [Check that the annotation shows](../agents/draw-for-a-presenter.md#check-that-the-annotation-shows).

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
