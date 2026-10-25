# Writing the Tiza docs

Read this page before you write or change a doc. It applies to the README, `docs/`, release notes, the CHANGELOG, help text, TUI and CLI messages, and MCP descriptions. The three roles that use it are in [Doc roles](doc-roles.md). The reasons for the rules are in [the documentation structure note](../research/documentation-structure.md).

## 1. Readers and forms

Tiza has four readers, in this order of priority:

1. A person who explains code and decides in 30 seconds if Tiza fits: a video, a talk, a review, a debug session with an LLM.
2. A person who uses Tiza and looks up a key, a `:` command or a flag.
3. A person or an agent that connects an AI agent through MCP or the socket.
4. A contributor, person or agent, who works on the repo.

Each page has one form (Diataxis calls them the four forms of documentation). Do not mix two forms on one page, except on a feature page that follows [Page order](#page-order). Link to the other form.

| Form | The reader wants to | Lives in |
|---|---|---|
| Tutorial | learn by doing, with a guide | `docs/getting-started.md` |
| How-to guide | do one task | `docs/guides/` |
| Reference | look up a fact | `docs/reference/` |
| Explanation | understand why | `docs/concepts/` |

- Agent pages go in `docs/agents/`, contributor pages in `docs/contributing/`. ADRs and research notes are not reader pages.

## 2. README rules

- The first screen answers four questions, in this order: what it is, who it is for, how to install it, the first command.
- Put a real Tiza render before the long text.
- The first command runs as written after each install route. A user step never needs a clone of the repository.
- Keep the README to 1,000 words of prose, 150 lines and 8 H2 sections. Everything else is a link.
- Keep the "Inspired by" line, the "Made by" line and the License section.
- Do not put maintainer steps in the README. Releases and the gate go in `docs/contributing/`.

## 3. Page templates

- Tutorial: the result in one sentence, what you need, numbered steps, a figure after each stage, "Where next".
- How-to guide: the goal in one sentence, what you need, numbered steps, the result as a figure, "See also".
- Reference: one sentence on what the page covers, then a table or a list. One row per item. No story.
- Explanation: the problem, the decision, the effect on the reader, a link to the ADR.
- Every page starts with one sentence that says what the page is for. A guide also says what Tiza is and links to install.
- Budgets: a guide or a concept page has 150 lines or fewer, and the tutorial 250. Split a reference page when one table covers two topics.
- Do not write "above" or "below" for another section: link to its heading. Link an ADR as "Decided in ADR 0007".

### Page order

A page about one feature uses this order. For that page, it replaces the template of its form. Put what the reader does first and how Tiza does it last. Leave out a part that the page does not need:

1. The H1, then one sentence on what the page is for. A second sentence can say what the page covers.
2. How to use it: the usual case, in short paragraphs or numbered steps.
3. The reference tables: commands, flags, messages and status bar states.
4. The special cases: what changes in each one. Give each case its own section or list item.
5. "How it works": the internals, in a short section at the end. When only a contributor needs them, move them to `docs/contributing/`.

[Saving](../concepts/saving.md) is the model page for this order.

Write a heading as a noun phrase for a topic, for example "Messages". Write it in the imperative for a task that the reader does, for example "Undo a reload".

### Limits

- Keep a paragraph to five sentences or fewer.
- Keep a list item to 60 words or fewer.
- Keep a table cell to one sentence.
- Use a table when you list more than three commands, flags, messages or states.

These limits are rules for the writer. A QA test does not check them.

### Tables, lists, code blocks and callouts

| Use | For |
|---|---|
| A table | items that have the same fields: a command and what it does, a message and when it shows |
| A numbered list | steps that the reader does in order, or events in the order that they occur |
| A bullet list | facts that do not have the same fields |
| A code block | text that the reader types, or sees exactly: a command, a file, a line on stderr |
| Backticks | a name in a sentence: a command, a key, a flag, a field, a message |
| A callout | a warning about lost work, at most one on a page |

- Write a callout as a quote with a bold label: `> **Warning:** …`. GitHub and mdBook both show it.
- Copy each message and each status bar state exactly as the code writes it. Write a placeholder in capitals: `FILE`, `PATH`, `N`, `ID`.
- Do not put a literal value in a message table. When a literal value helps, put it in a code block after a line that starts with "For example".
- A message table lists every message of the page. In the text, quote a message only where it explains its case.

## 4. Voice and glossary

Write all public text in English, in ASD-STE100 Simplified Technical English. Chat and internal notes can be in another language.

- Write one idea per sentence.
- Use 20 words or fewer in an instruction and 25 or fewer in a description.
- Use the active voice. Use the imperative for an instruction.
- Write "you" for the reader. In agent pages, write "the presenter" for the person and "you" for the agent.
- Do not use idioms, marketing adjectives, emphasis in capitals or dash asides. Use a colon or a new sentence.
- Do not copy the voice of another project. Copy its structure only.
- Use straight double quotes for a quote. Do not use «» or curly quotes.
- A bold label can start a list item only when a full sentence follows it.
- Commands, JSON fields, file formats and product names are technical names. Do not change them.
- Use one word for one concept. Use the glossary below. A new term goes into the glossary first.

| Use | For | Do not use |
|---|---|---|
| annotation | anything Tiza draws on the code: mark, highlight, box, arrow, note, callout, spotlight | drawing, overlay |
| document | the `.tiza` file (JSON) that holds the code reference, the annotations and the steps | sidecar, scene (except in `tiza scene`, `scene.get`, `tiza://scene`) |
| step | one numbered state of a document; step 0 shows only the code | slide, frame |
| take | the layout that presentation mode and the exports use for all the steps of a document | recording, sequence |
| frame | one image of one step in an export | step |
| export | a file that `tiza render` or `export_image` writes: text, Markdown, SVG, PNG, GIF and the others | snapshot |
| presentation mode | the mode that hides the interface (`P`, `--present`) | take mode |
| normal mode | the mode where you write annotations | authoring screen |
| move | a change that a step makes to the code or the view (`code.patch`, `code.link`, `cursor`, `select`, `wait`) | patch (except `code.patch`) |
| view | one file that a `code.link` shows, with its own annotations | pane (Herdr panes keep their name) |
| mark | the annotation that puts a glyph and a legend on a span | badge, tag |
| kind | the name of a mark, built in or custom (`warn`, `read`, `io`) | type |
| style | a custom kind that you define (`:style`, `define_style`) | theme |
| anchor | the code text that an annotation follows | position, cell |
| span | the part of a line that a highlight, a mark or an arrow end covers | range (except a line range) |
| instance | the running TUI that agents talk to | server, app |
| color | the color of an annotation or a theme token | colour |
| 24-bit color | the terminal feature that shows any RGB color | truecolor, RGB |
| status bar | the bottom row of normal mode, with the state and the last message | status line, the bar, message line |
| message | the text that a command shows in the status bar | notice, toast |
| presenter | the person who shows the code; only in agent pages | user |
| code reference | code that the document reads from a file (`"code": {"file": …}`) | file link, pointer |
| embedded code | code that the document holds as a copy (`:embed`) | inline code, frozen code |
| follow mode | the mode that reloads the code when the file changes on disk (`--follow`, `:follow`) | watch mode, live reload |
| autosave | the save of each change to the document when you make it (`:autosave`) | auto-save, autosaving |
| no-save mode | the mode that never saves the document by itself (`--no-save`) | dry run, read-only mode |
| read-only document | a `.tiza` file that Tiza cannot write: the file or its folder is read-only | locked file |

## 5. Numbers

- Do not type a volatile number by hand: a size, a count, a version, a time.
- Let a script write it, or let a test check it. Otherwise write "about" and link to the source: `qa/budgets.json`, a release page.
- A constant of the code (a timeout, a limit) can stay. A QA test must check it.
- A "tested with" line (a version and a date) is a volatile number. Check it again at each release.

## 6. Visuals

- Every Tiza frame is a real `tiza render` export. Do not retouch it. Do not make a mockup.
- The brand serif (Libron) is only for titles around a frame, never inside it.
- Do not publish an image that an AI made. Do not use the chalk texture background. Keep screenshots and QA evidence out of the repo.
- Use a GIF for a flow and a PNG for a still. Use a text render (`--format md`) when the reader copies or searches.
- A frame of a snippet: at most 53 columns, `--font-size 13 --scale 2`, shown with `<img width="466">`.
- A frame of a full step: 100 columns, `--font-size 12 --scale 2`, shown with `<img width="768">`.
- A tour GIF: 100 columns (106 to 130 if a label needs it), `--font-size 12 --scale 2`.
- Markdown image syntax cannot set a width. Use an `<img>` tag with `width`, `alt` and `src`.
- On a reference page, add the `md` render of the step under the image, in a `<details>` block.
- Alt text says what to notice. A caption says which example and which step.
- Make every figure with a script. A second run must give the same bytes.

## 7. Links

- Inside `docs/`, use relative links. They work on GitHub and in mdBook.
- In README.md, link to a page outside `examples/` with an absolute URL. The release archive holds the binary, the README, the two licenses and `examples/`.
- The docs site is https://tiza.luisk.dev. Until it exists, link to the page on GitHub.
- The text of a link names the page or the section: `[Saving](../concepts/saving.md)`, not the file name.
- A link to a section names the section: `see [Moves](reference/document-format.md#moves)`, not "see here".
- Do not link to the board, to a session or to anything private. The repo is public.
- Do not write a ticket id in a public file. A QA test fails on it.

## 8. Keeping it true

- A commit that changes behavior also changes its page. The structure note has the change-to-page map.
- Run every command that you write against the built binary. List the commands in the commit message.
- Check every claim against the code or a test. Never write a flag, a key or a number from memory.
- QA tests check behavior, contracts with scripts, generated images, links and repo policies. A test can compare a doc with the code. A test never checks the prose of a doc. [AGENTS.md](https://github.com/luiscarlosjayk/tiza/blob/main/AGENTS.md) gives the full rule.
- Before you move text, find the tests that read it: an anchor, a link or a line that a script writes. Change them in the same commit. A change to what a test checks needs the owner.
- Move text verbatim in one commit and edit it in the next. Then `git log --follow` and the review stay simple.
- Text that you edit changes to this style. Do not change other text in the same commit.
- Put the notice "Not released yet" on a page about a feature that is in no release. The version bump commit removes it.
- Write commit messages in English, in the imperative, with a subject of 72 characters or fewer.

## 9. Rewriting a page

Do these steps before you send a rewrite for review:

1. Make a fact map. List each claim of the old text, and write where it is in the new text.
2. Do not lose a fact, and do not change one. A new fact needs a check in the binary or in the code.
3. Run each command of the page against the built binary, in a pseudo-terminal. Check each other claim against the code or a test.
4. Write down the claims that you could not check.
5. Find the links and the anchors to the page. Keep each anchor, or change each link in the same commit.
6. Look at the page at desktop width and at 390 pixels, in a GitHub-like preview.
7. Check the style: this guide, the glossary and the spelling "color".

Keep the fact map and the preview out of the repo. Send them with the change to the person who reviews it.
