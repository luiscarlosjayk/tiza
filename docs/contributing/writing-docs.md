# Writing the Tiza docs

Read this page before you write or change a doc. It applies to the README, `docs/`, release notes, the CHANGELOG, help text, TUI and CLI messages, and MCP descriptions. The three roles that use it are in [doc-roles.md](doc-roles.md). The reasons for the rules are in [the documentation structure note](../research/documentation-structure.md).

## 1. Readers and forms

Tiza has four readers, in this order of priority:

1. A person who explains code and decides in 30 seconds if Tiza fits: a video, a talk, a review, a debug session with an LLM.
2. A person who uses Tiza and looks up a key, a `:` command or a flag.
3. A person or an agent that connects an AI agent through MCP or the socket.
4. A contributor, person or agent, who works on the repo.

Each page has one form (Diataxis calls them the four forms of documentation). Do not mix two forms on one page. Link to the other form.

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
- Keep the "Inspired by" line, the "Made by" line and the License section as they are. A QA test pins them.
- Do not put maintainer steps in the README. Releases and the gate go in `docs/contributing/`.

## 3. Page templates

- Tutorial: the result in one sentence, what you need, numbered steps, a figure after each stage, "Where next".
- How-to guide: the goal in one sentence, what you need, numbered steps, the result as a figure, "See also".
- Reference: one sentence on what the page covers, then a table or a list. One row per item. No story.
- Explanation: the problem, the decision, the effect on the reader, a link to the ADR.
- Every page starts with one sentence that says what the page is for. A guide also says what Tiza is and links to install.
- Budgets: a guide has 150 lines or fewer, a concept page 80 and the tutorial 250.
- Keep a paragraph to five sentences or fewer. Keep a table cell to one sentence.
- Do not write "above" or "below" for another section: link to its heading. Link an ADR as "Decided in ADR 0007".

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
| frame | one image of one step in an export | step |
| presentation mode | the mode that hides the interface (`P`, `--present`) | take mode |
| normal mode | the mode where you write annotations | authoring screen |
| move | a change that a step makes to the code or the view (`code.patch`, `code.link`, `cursor`, `select`, `wait`) | patch (except `code.patch`) |
| view | one file that a `code.link` shows, with its own annotations | pane (Herdr panes keep their name) |
| mark | the annotation that puts a glyph and a legend on a span | badge, tag |
| kind | the name of a mark, built in or custom (`warn`, `read`, `io`) | type |
| style | a custom kind that you define (`:style`, `define_style`) | theme |
| anchor | the code text that an annotation follows | position, cell |
| instance | the running TUI that agents talk to | server, app |
| color | the color of an annotation or a theme token | colour |
| 24-bit color | the terminal feature that shows any RGB color | truecolor, RGB |
| status bar | the bottom row of normal mode | status line, the bar |
| presenter | the person who shows the code; only in agent pages | user |

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
- A link to a section names the section: `see [Moves](reference/document-format.md#moves)`, not "see here".
- Do not link to the board, to a session or to anything private. The repo is public.
- Do not write a ticket id in a public file. A QA test fails on it.

## 8. Keeping it true

- A commit that changes behavior also changes its page. The structure note has the change-to-page map.
- Run every command that you write against the built binary. List the commands in the commit message.
- Check every claim against the code or a test. Never write a flag, a key or a number from memory.
- Before you move text, find the tests that read it. The structure note lists them per doc. Move the test in the same commit.
- When a test pins a sentence that you edit, change the test in the same commit. A change to what a test checks needs the owner.
- Move text verbatim in one commit and edit it in the next. Then `git log --follow` and the review stay simple.
- Text that you edit changes to this style. Do not change other text in the same commit.
- Put the notice "Not released yet" on a page about a feature that is in no release. The version bump commit removes it.
- Write commit messages in English, in the imperative, with a subject of 72 characters or fewer.
