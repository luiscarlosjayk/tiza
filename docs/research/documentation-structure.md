# Documentation structure

This note measures the Tiza documentation at v0.1.0 and proposes a target structure. The owner approves or changes the proposal. Nothing in this note is done yet: no existing doc changes here.

Two pages come with it: [the writing guide](../contributing/writing-docs.md) and [the three doc roles](../contributing/doc-roles.md).

## 1. Status and method

- Measured at commit `ee7dbda` (tag `v0.1.0`), on 2026-10-25. README line numbers in this note are at `ee7dbda`.
- The README was measured again at `4d66ac0`, the commit that puts `install.sh` first (§2.3). That commit adds 22 lines after line 43: add 22 to a README line number after line 55.
- Start condition: all the features of the v0.1 milestone are in v0.1.0. The work that is still open only waits for the owner's test.
- Every number in sections 2 to 8 comes from a command. Each section gives its command.
- Work packages have names, not board ids. The board is private.

How the counts were made:

- `wc -lw` for lines and words.
- Prose words: the text without fenced code blocks, HTML comments and `<img>` tags. The README text block between the `readme-images` markers is inside a fence, so it is not counted. Tables count as prose.
- First runnable command: the first fenced block in `sh`, `bash`, `console` or `shell`, or whose first line starts with `tiza`, `cargo`, `curl`, `git` or `sh`.

## 2. Measurements

### 2.1 Size of each doc

`wc -lw README.md AGENTS.md CONTRIBUTING.md SECURITY.md CHANGELOG.md docs/*.md docs/*/*.md qa/README.md examples/README.md crates/tiza-cli/README.md`:

| File | Lines | Words |
|---|---:|---:|
| README.md | 322 | 9,592 |
| docs/agent-guide.md | 172 | 8,385 |
| docs/demos/scenarios.md | 241 | 4,181 |
| docs/ui-guidelines.md | 252 | 2,709 |
| CHANGELOG.md | 163 | 5,031 |
| qa/README.md | 109 | 2,886 |
| docs/zed.md | 83 | 1,159 |
| AGENTS.md | 36 | 1,103 |
| docs/release-notes/v0.1.0.md | 52 | 672 |
| examples/README.md | 43 | 640 |
| crates/tiza-cli/README.md (the crates.io page) | 40 | 328 |
| SECURITY.md | 41 | 292 |
| CONTRIBUTING.md | 43 | 280 |
| docs/brand/README.md | 24 | 240 |

`ls -la docs/img docs/img/src`: `docs/img` holds 1,720 KB. `hero.png` is 671 KB, `ownership.gif` 387 KB and `ownership-step7.png` 216 KB. The sources in `docs/img/src` hold three step PNGs (468 KB) and `hero.html`. `docs/brand` holds 444 KB.

### 2.2 The README

- 8,146 words of prose, 13 H2 sections and 2 H3 sections.
- 124 of 322 lines are longer than 100 characters. The longest line has 4,042 characters (line 135, "Follow mode").
- The first runnable command is `cargo install tiza` at line 55. 378 words of prose come before it.

Prose words per section:

| Section | Prose words |
|---|---:|
| (before the first H2) | 204 |
| Installation | 232 |
| Quick start (with Keys and `:` commands) | 3,184 |
| Export to text, Markdown, ANSI, SVG, PNG, GIF, MP4, asciicast or VHS | 1,147 |
| For agents: MCP and local API | 879 |
| Terminal support | 613 |
| Themes | 567 |
| Recommended recording profile | 519 |
| Releases (maintainers) | 419 |
| What Tiza reads and writes | 224 |
| License | 54 |
| Development | 46 |
| Feedback and bugs | 44 |
| Zed integration | 14 |

The "Quick start" section is 39 % of the README. It holds the full reference for keys and `:` commands, and long paragraphs on follow mode, saving, moves, views and named steps.

### 2.3 The README at 4d66ac0

The install commit changed only the "Installation" section (one hunk, old lines 43-55, new lines 43-77).

| Measure | `ee7dbda` | `4d66ac0` |
|---|---:|---:|
| Lines | 322 | 344 |
| Words | 9,592 | 9,710 |
| Prose words | 8,146 | 8,237 |
| Lines over 100 characters | 124 | 127 |
| Prose words in "Installation" | 232 | 323 |
| First runnable command | `cargo install tiza`, line 55 | `curl … install.sh \| sh`, line 49 |
| Prose words before it | 378 | 223 |

The new "Installation" section, for the inventory (lines at `4d66ac0`):

| Line | Paragraph | Reader | Form | Destination | Action | Work package |
|---|---|---|---|---|---|---|
| 46-52 | The `install.sh` command and what it does | 1 | how-to | README install | stay | new README |
| 54-66 | `TIZA_VERSION`, `TIZA_INSTALL_DIR`, read the script first | 1 | how-to | `guides/install.md` | move | tutorial and guides |
| 68 | Update, watch releases | 2 | how-to | `guides/install.md` | move | tutorial and guides |
| 70 | By hand: archives, glibc, sizes, Windows | 1 | how-to | `guides/install.md` | move; the size "6 to 7 MB" is typed by hand (§6.2) | tutorial and guides |
| 72 | macOS quarantine | 1 | how-to | `guides/install.md` | move | tutorial and guides |
| 74-79 | From source with `cargo install` | 1 | how-to | README install | stay | new README |

These rows replace the rows for old lines 46 to 57 in §4.1.

### 2.4 The agent guide and the other pages

- `docs/agent-guide.md`: 20 H2 sections. The first command block is at line 166, after 8,198 words of prose. The longest line has 4,461 characters.
- `AGENTS.md`: one H1 and two H2 sections ("Working the board", "Pushing").
- `qa/README.md`: one H1 and four H2 sections.
- `examples/README.md`: three H2 sections. Five examples, all in Rust.
- `docs/zed.md`: three H2 sections.

## 3. Readers and positioning

### 3.1 Positioning (decided)

The positioning sentence is "Draw on top of code in your terminal to explain, debug and teach." It is in the README, the crate README, `crates/tiza-cli/Cargo.toml`, `tiza --help`, the MCP instructions and the release notes. A QA test requires the same sentence in all of them.

Three places keep the old sentence "step-by-step code annotations for the terminal". Two are the alt text of the hero, in README.md line 1 and in crates/tiza-cli/README.md line 3. The third is the text inside `hero.png`, made from `docs/img/src/hero.html` line 89. Make the hero again with `scripts/readme-images.sh`.

Three differentiators, for the new README:

- Annotations stay on their code text when you edit, scroll or resize.
- AI agents draw on the same code, live, through MCP or a local socket.
- The `tiza` binary never opens a network connection.

### 3.2 Readers in priority order (proposal)

1. A person who explains code and decides in 30 seconds if Tiza fits.
2. A person who uses Tiza and looks up a key, a `:` command or a flag.
3. A person or an agent that connects an AI agent through MCP or the socket.
4. A contributor, person or agent, who works on the repo.

The first screen of the README serves reader 1 only. Readers 2 to 4 get one link each.

## 4. Inventory

The columns are the reader, the Diataxis form, the destination, the action and the work package. The reader is 1 to 4 from §3.2, or R for a person who records a video. The actions are **stay** (in the README), **move** (verbatim, then edit), **merge** (into a page with the same fact), **rewrite** and **drop** (with the reason).

A row with two forms is split into two rows. The work packages are in §9.

### 4.1 README.md

| Line | Section or paragraph | Reader | Form | Destination | Action | Work package |
|---|---|---|---|---|---|---|
| 1-15 | Hero, positioning, intro, agents paragraph, the GIF, status, "Inspired by" | 1 | landing | README | rewrite: keep the sentence, the status line and "Inspired by" | new README |
| 17-40 | The text block between the `readme-images` markers | 1 | landing | README | stay | new README |
| 44 | "written in Rust with ratatui" | 4 | landing | README, contributing section | move | new README |
| 46 | Binaries, download sizes, Windows, release notes | 1 | how-to | README install, then `guides/install.md` | rewrite: `install.sh` first; drop the sizes (volatile, see §6) | new README |
| 48 | macOS quarantine | 1 | how-to | `guides/install.md`, one line in README | move | tutorial and guides |
| 50 | Watch releases on GitHub | 2 | how-to | `guides/install.md` | move | tutorial and guides |
| 52-57 | `cargo install` | 1 | how-to | README install | stay | new README |
| 59 | Internal crates have no stable API | 4 | reference | `contributing/releasing.md` and the crate README | move | new README |
| 61 | `rust-toolchain.toml` pins the toolchain | 4 | reference | `contributing/development.md` | move | repo conventions |
| 63-75 | Quick start commands, the bundled demo | 1 | tutorial | README quick start (three steps), `getting-started.md` | rewrite: code that the reader copies from the page, no `examples/` (decision row 16) | new README, tutorial and guides |
| 77-90 | Keys table | 2 | reference | `reference/keys.md` | move | reference pages |
| 88 | Keys table, `f` row: how the live pointer behaves | 2 | how-to | `guides/present.md` | rewrite: one row in keys, the rest in the guide | tutorial and guides |
| 92-129 | `:` commands block | 2 | reference | `reference/commands.md` | move, then add the 7 commands it omits (§5) | reference pages |
| 131 | Language detection | 2 | reference | `reference/cli.md` | move | reference pages |
| 133 | New annotations, the `.tiza` file, `:embed`, `:ref`, color, arrow styles | 2 | reference | `reference/document-format.md` | rewrite | reference pages |
| 133 | Same paragraph: how to color and style annotations | 1 | tutorial | `getting-started.md` | rewrite | tutorial and guides |
| 135 | Follow mode: the commands and the status line | 2 | how-to | `guides/follow-the-editor.md` | rewrite | tutorial and guides |
| 135 | Follow mode: how annotations follow their text | 2 | explanation | `concepts/anchors.md` | rewrite | concept pages |
| 137 | Saving: autosave, `--no-save`, `:w`, `:saveas`, read-only files | 2 | reference | `reference/commands.md` and `reference/cli.md` | rewrite | reference pages |
| 137 | Saving: "no way out writes anything the mode would not" | 2 | explanation | `concepts/saving.md` | rewrite | concept pages |
| 139 | Moves: the fields and targets | 2 | reference | `reference/document-format.md` | move | reference pages |
| 139 | Moves: why the document is computed again from step 0 | 2 | explanation | `concepts/steps-and-moves.md` | rewrite | concept pages |
| 141 | Several files (views): how to link a file | 2 | how-to | `guides/several-files.md` | rewrite | tutorial and guides |
| 141 | Views: `cursor`, `select`, `wait` | 2 | reference | `reference/document-format.md` | move | reference pages |
| 143 | Named steps, auto-advance and loop: the fields | 2 | reference | `reference/document-format.md` | move | reference pages |
| 143 | Named steps: `tiza goto`, `next`, `prev` | 2 | how-to | `guides/present.md` | rewrite | tutorial and guides |
| 145-158 | Export commands | 2 | reference | `reference/exports.md` | move | reference pages |
| 145-158 | Export: which format for which use | R | how-to | `guides/record-gif-or-video.md` | rewrite | tutorial and guides |
| 160 | `--all-steps` | R | reference | `reference/exports.md` | move | reference pages |
| 162 | Stable geometry | R | explanation | `concepts/stable-geometry.md` | rewrite | concept pages |
| 164-171 | GIF, MP4, cast, tape | R | reference | `reference/exports.md` | move | reference pages |
| 173-175 | Text formats and options, fonts | 2 | reference | `reference/exports.md` | move | reference pages |
| 177-179 | Step-7 PNG and "every image is made by Tiza" | 1 | landing | README | rewrite: one sentence near the GIF | new README |
| 181-195 | MCP setup | 3 | how-to | README "For agents" (short), `agents/setup.md` | rewrite | agent docs |
| 195 | MCP tool list, anchors, resources | 3 | reference | `reference/mcp-tools.md` | merge with the agent guide; the list comes from `tools/list` | reference pages, MCP tool families |
| 197 | Mouse | 2 | how-to | `guides/present.md` | move | tutorial and guides |
| 199 | Supervised mode | 3 | how-to | `agents/supervised-mode.md` | merge with agent guide line 99 | agent docs |
| 201 | What an agent adds where you cannot see it | 3 | reference | `agents/using-tiza.md` | merge | agent docs |
| 203 | After rebuilding Tiza | 4 | how-to | `agents/using-tiza.md` | merge with agent guide line 153 | agent docs |
| 205 | JSON Schema, stdin | 3 | reference | `reference/socket-api.md` | merge with agent guide line 157 | reference pages |
| 207-221 | Socket and CLI | 3 | reference | `reference/socket-api.md`, `reference/cli.md` | merge with agent guide line 161 | reference pages |
| 223-227 | What Tiza reads and writes | 1 | explanation | `concepts/privacy-and-network.md`, four lines in README | rewrite | concept pages, new README |
| 229-231 | Zed integration | 2 | how-to | `guides/zed.md` | stay as one link | new README |
| 233-246 | Terminal support: requirements | 2 | reference | `reference/terminals.md` | move | reference pages |
| 247-251 | Known caveats (Terminal.app, tmux, Zed) | 2 | how-to | `reference/terminals.md`, a "Problems" section | rewrite | reference pages |
| 253 | Report a terminal problem | 2 | how-to | `reference/terminals.md` and README feedback | merge with the feedback section | new README |
| 255-262 | Themes: how to choose and change | 2 | how-to | `guides/themes.md` | rewrite | tutorial and guides |
| 261-262 | Theme tokens and file format | 2 | reference | `reference/themes.md` | move | reference pages |
| 264-266 | `tiza check` before you record | R | how-to | `guides/check-before-you-record.md` | rewrite | tutorial and guides |
| 266 | `tiza check` findings and `--json` | R | reference | `reference/cli.md` | move | reference pages |
| 268 | Recording profile: font, width, legibility | R | how-to | `guides/record-gif-or-video.md` | rewrite | tutorial and guides |
| 270-280 | Development | 4 | how-to | `contributing/development.md` | merge with AGENTS.md (the gate) | agent docs |
| 282-310 | Releases (maintainers) | 4 | how-to | `contributing/releasing.md` | move | agent docs |
| 312-314 | Feedback and bugs | 1 | landing | README | stay | new README |
| 316-322 | License, contribution clause, "Made by" | 1 | landing | README | stay (a QA test pins them) | new README |

### 4.2 docs/agent-guide.md

| Line | Section | Reader | Kind | Destination | Action | Work package |
|---|---|---|---|---|---|---|
| 1-10 | Title, mental model | 3 | explanation | `agents/using-tiza.md` | move | agent docs |
| 11-25 | Recommended flow | 3 | how-to | `agents/using-tiza.md` | move, then split the long steps | agent docs |
| 26-37 | Example (ownership) | 3 | tutorial | `agents/using-tiza.md`, examples | move | agent docs, examples |
| 38-45 | Emphasis, Color | 3 | how-to | `agents/using-tiza.md` | move | agent docs |
| 46-51 | Fixing a misplaced annotation | 3 | how-to | `agents/using-tiza.md` | move | agent docs |
| 52-55 | Is it on screen? | 3 | reference | `agents/using-tiza.md` | move | agent docs |
| 56-65 | Pointing live, Between takes | 3 | how-to | `agents/using-tiza.md` | move | agent docs |
| 66-73 | While the presenter edits | 3 | explanation | `concepts/anchors.md` (shared), link from the agent page | merge with README line 135 | concept pages |
| 74-79 | Code that changes between steps (moves) | 3 | reference | `reference/document-format.md`, link from the agent page | merge with README line 139 | reference pages |
| 80-85 | Showing another file (views) | 3 | reference | `reference/document-format.md` | merge with README line 141 | reference pages |
| 86-98 | Named steps, auto-advance and loop | 3 | reference | `reference/document-format.md` | merge with README line 143 | reference pages |
| 99-116 | Supervised mode | 3 | how-to | `agents/supervised-mode.md` | move | agent docs |
| 117-144 | Resources | 3 | reference | `reference/mcp-tools.md` | move | reference pages |
| 145-148 | Idempotency | 3 | reference | `reference/socket-api.md` | move | reference pages |
| 149-152 | Finding the tools (deferred tools) | 3 | how-to | `agents/setup.md` | move | agent docs |
| 153-156 | After a rebuild | 3 | how-to | `agents/setup.md` | merge with README line 203 | agent docs |
| 157-160 | Schema | 3 | reference | `reference/socket-api.md` | merge with README line 205 | reference pages |
| 161-172 | Without MCP: socket and CLI | 3 | reference | `reference/socket-api.md` | merge with README lines 207-221 | reference pages |

### 4.3 AGENTS.md, qa/README.md, examples/README.md, docs/zed.md

| File and line | Section | Reader | Kind | Destination | Action | Work package |
|---|---|---|---|---|---|---|
| AGENTS.md 1-16 | Rules for agents in the repo | 4 | reference | AGENTS.md | rewrite: commands, testing, structure, style, git, boundaries, an index | agent docs, repo conventions |
| AGENTS.md 17-28 | Working the board | 4 | how-to | AGENTS.md | stay: it names no private URL | agent docs |
| AGENTS.md 29-36 | Pushing, releases | 4 | how-to | AGENTS.md, link to `contributing/releasing.md` | rewrite | agent docs |
| qa/README.md 1-28 | QA tests, one file per task | 4 | reference | `qa/README.md` | stay (decision row 9) | none |
| qa/README.md 29-49 | Terminal size matrix | 4 | how-to | `qa/README.md` | stay | none |
| qa/README.md 50-53 | MCP context trials | 4 | reference | `docs/research/mcp-context.md` | merge | none |
| qa/README.md 54-91 | Size breakdown of the first baseline | 4 | explanation | `qa/README.md` | stay | none |
| qa/README.md 92-109 | Checklist for a new `:` command | 4 | how-to | `contributing/development.md` | move | agent docs |
| examples/README.md 1-12 | The examples table | 1 | reference | `docs/examples/README.md`, the gallery on the site | rewrite: one row per example, with its language, its figure and its code inline | examples |
| examples/README.md 13-29 | The three `code` modes | 2 | reference | `reference/document-format.md` | move | reference pages |
| examples/README.md 30-40 | Custom mark styles | 2 | reference | `reference/document-format.md`, examples | move | reference pages |
| examples/README.md 41-43 | Themes | 2 | reference | `guides/themes.md` | merge | tutorial and guides |
| docs/zed.md 1-56 | Zed tasks | 2 | how-to | `guides/zed.md` | move (`git mv`), then add numbered steps and a result | tutorial and guides |
| docs/zed.md 57-74 | Driving the demo from Zed | 2 | how-to | `guides/zed.md` | move | tutorial and guides |
| docs/zed.md 75-83 | Gotchas | 2 | how-to | `guides/zed.md` | move | tutorial and guides |

Other pages:

- `docs/ui-guidelines.md` goes to `docs/contributing/ui-guidelines.md` (`git mv`).
- `docs/demos/scenarios.md` goes to `docs/agents/scenarios.md` and feeds the agent examples.
- `docs/release-notes/`, `docs/brand/`, `docs/adr/` and `docs/research/` stay where they are.
- CONTRIBUTING.md and SECURITY.md stay at the root: GitHub shows them in its interface from there (decision row 9).

## 5. Docs inside the binary

| Surface | Source | Size | Copies that must follow it |
|---|---|---|---|
| `tiza --help` and the help of each subcommand | `crates/tiza-cli/src/main.rs` (clap) | 606 words for `tiza --help`; 1,701 for all | `reference/cli.md`, README quick start |
| TUI help (`?`) | `keys::HELP` in `crates/tiza-tui/src/keys.rs` (7 sections) | 418 lines of source | `reference/keys.md`, README keys table |
| `:` command names | `App::COMMANDS` in `crates/tiza-tui/src/app.rs` | 44 commands | `reference/commands.md`, README `:` block |
| MCP server instructions | `get_info` in `crates/tiza-mcp/src/lib.rs` | 1,027 characters | README "For agents", `agents/using-tiza.md` |
| MCP tool and parameter descriptions | `crates/tiza-mcp/src/lib.rs` | 30 tools; `tools/list` is 37,227 bytes; descriptions 10,954 characters | `reference/mcp-tools.md`, README tool list |
| MCP resources | `crates/tiza-mcp/src/lib.rs` | 4 resources | `reference/mcp-tools.md` |
| JSON Schemas | `tiza schema document` and `tiza schema api` | 33,570 and 220,245 bytes | `reference/document-format.md`, `reference/socket-api.md` |

Findings:

- The README `:` block omits 12 of the 44 commands. Five of them are in the prose (`:embed`, `:link`, `:list`, `:ref`, `:wq`). The README never names the other 7: `:help`, `:next`, `:prev`, `:present`, `:title`, `:undo` and `:redo`.
- No error message points to a doc. So no message breaks when a page moves.
- The longest MCP descriptions are `add_move` (1,466 characters), `check` (1,315) and `update_annotation` (866).

The rule: the code is the source. A page copies it, and a QA test compares the page with the code. The reference pages work package writes these tests: `App::COMMANDS` against `commands.md`, the clap options against `cli.md`, `tools/list` against `mcp-tools.md`, and `api.schema.json` against `socket-api.md`.

## 6. Duplicated facts and volatile numbers

### 6.1 Facts in two places

| Fact | Places | Home |
|---|---|---|
| MCP tool list | README 195, agent guide (flow), `tools/list` | `tools/list`; `reference/mcp-tools.md` copies it with a test |
| Supervised mode | README 199, agent guide 99-116 | `agents/supervised-mode.md` |
| Reconnect after a rebuild | README 203, agent guide 153 | `agents/setup.md` |
| Moves, views, named steps | README 139-143, agent guide 74-98 | `reference/document-format.md` |
| Follow mode and editing | README 135, agent guide 66-73 | `concepts/anchors.md` and `guides/follow-the-editor.md` |
| MCP resources | README 195, agent guide 117-144 | `reference/mcp-tools.md` |
| Schema and socket | README 205-221, agent guide 157-172 | `reference/socket-api.md` |
| Keys | README 77-90, TUI help (`keys::HELP`) | `keys::HELP`; `reference/keys.md` with a test |
| `:` commands | README 92-129, TUI help, `App::COMMANDS` | `App::COMMANDS`; `reference/commands.md` with a test |
| Export options | README 175, `tiza render --help` | clap; `reference/exports.md` with a test |
| Zed `line_height` | README 251, docs/zed.md | `guides/zed.md` |
| OSC color query | README 244 and 259 | `reference/terminals.md` |
| Legibility gate (4.5:1) | README 257 and 268, agent guide | `guides/themes.md` |
| The gate commands | README 272-278, AGENTS.md (twice) | AGENTS.md |
| Feedback channels | README 253 and 314, CONTRIBUTING.md, release notes | CONTRIBUTING.md |
| Install and quick start | README, crate README, release notes | README; the crate README links to it |
| No network | README 225-227, SECURITY.md, ADR 0009 | `concepts/privacy-and-network.md`; ADR 0009 is the decision |
| Positioning sentence | 6 places | the README; a QA test keeps the copies the same |

### 6.2 Volatile numbers typed by hand

| Number | Where | Today | What can keep it true |
|---|---|---|---|
| Download size and binary size | README 46 (at `4d66ac0`: line 70, "6 to 7 MB", true today) | At `ee7dbda` the README says 6.3 MB for Linux and 5.7 MB for macOS arm64. The v0.1.0 archives are 6,985,273 and 6,290,543 bytes. The macOS binary is 12,284,640 bytes. | Drop it from the README. Link to the release page. `qa/budgets.json` keeps the limits. |
| Number of MCP tools | README 195 (a list), an older research note (31) | 30 tools | Generate the list, or a test against `tools/list` |
| GIF size of the ownership example | README 166 | The README says ~395 kB and 2048×1650 px. A render at 100 columns gives 385,198 bytes and 2048×1692 px. | Write "about 400 kB" and link to the example, or let `readme-images.sh` write it |
| Narrowest clean width | README 266 ("91") | 91 | A QA test that runs `tiza check` on the example |
| Rust versions | README 55 and 61 | `rust-version = "1.88"`, `channel = "1.99.1"` | A QA test that reads `Cargo.toml` and `rust-toolchain.toml` |
| Version in the status line | README 15 | 0.1.0 | `scripts/bump-version.sh` already keeps it true |
| Timeouts and limits (120 ms, 60 s, 100 ms, 50 lines) | README 127, 135, 199, 244, 259 | constants of the code | A QA test per constant, or remove them from the guides |

## 7. Tests, scripts and workflows that read the docs

This table is the contract for the work packages that move text. Before you move a sentence, find its row. Move the assertion in the same commit.

Found with `git grep -n "README\|agent-guide\|docs/\|CHANGELOG" qa/tests scripts crates .github`, then read in context. One reader is one test function or one script step. The full table names each test and its function. QA test names carry board ids, so that table is outside the repo for now (decision row 13).

| Doc | Readers | What they pin | Where the assertion goes when the text moves |
|---|---:|---|---|
| README.md | 39 | About 30 exact phrases of "Terminal support"; the Ctrl-C row of the keys table (read at compile time); the reconnect note; the mouse paragraph; the saving phrases; the export scale phrases; the 14 release commands in order; the download size line | Each phrase moves to the test of its new page in the same commit |
| README.md, structure | (same) | Line 1 is the hero image `![Tiza: …](docs/img/hero.png)`. A line equal to the bold positioning sentence. One `> Status: **v…**` line. The `readme-images` markers and the text block between them, byte for byte. `## Releases (maintainers)` with its anchor, which `release.yml` names. `## License` is the last H2 and the "Made by" line is the last line. "Inspired by" with five links. | These stay in the README. The new README keeps the order: hero first, License last |
| CHANGELOG.md | 29 | One exact sentence per change; `## [Unreleased]` for `bump-version.sh` | The CHANGELOG does not move. Old entries keep their text |
| docs/agent-guide.md | 10 | Long exact sentences in "Recommended flow" and "Supervised mode"; the headings "After a rebuild" and "Is it on screen?" | Move each test with its sentence to `agents/using-tiza.md` or `agents/supervised-mode.md` |
| docs/release-notes/v0.1.0.md | 7 | The positioning sentence, the archive names, the install steps, no ticket id | Release notes do not move |
| docs/zed.md | 6 | About 15 exact phrases, the JSON tasks (they must parse, with fixed labels), a real CLI error message, the tested Zed and macOS versions | Change the path in the test when the page moves to `guides/zed.md` |
| docs/demos/scenarios.md | 6 | 10 to 15 numbered scenarios with fixed fields; every path it names exists | Change the path when the page moves to `agents/` |
| docs/img and docs/brand | 10 | The README images equal a fresh render; size caps; the hero HTML holds the old sentence "Step-by-step code annotations for the terminal" | A new hero changes this test in the same commit |
| AGENTS.md | 3 | The README images rule; the two budget scripts; a line that starts with `- No ticket ids` | Keep these lines when AGENTS.md is rewritten |
| CONTRIBUTING.md, SECURITY.md | 2 | Exact headings, a supported-versions row with exact spaces, links that resolve | These files stay at the root |
| crates/*/README.md | 3 | The crate README has absolute links and the positioning sentence; the internal crates say "no stable API" | No move |
| ADR 0004, 0006, 0007, 0009 | 5 | One exact sentence or table row each; the file names | ADRs do not move |
| qa/README.md | 1 | The Herdr matrix options | Stays in `qa/` |
| All tracked files | 2 | No board id outside `qa/`; the author name always in full | Applies to every new page |

Scripts and workflows:

- `release.yml`, `scripts/release-local.sh` and `scripts/size_check.py` put README.md, the two licenses and `examples/` in the release archive. Nothing from `docs/` ships.
- `scripts/readme-images.sh` writes the README images and the text block. `scripts/bump-version.sh` writes the status line and the CHANGELOG heading. `scripts/release-notes.sh` reads `docs/release-notes/`.

Links that no test checks:

- `crates/tiza-cli/README.md` links to `docs/img/hero.png`, `docs/agent-guide.md` and `examples/` on `main`.
- `.github/ISSUE_TEMPLATE/bug.yml` links to `CONTRIBUTING.md` on `main`.
- A move of `docs/agent-guide.md` or `docs/img/hero.png` breaks the crates.io page with no test failure. The docs checks must cover absolute links to the repo.

## 8. Cold read of the README (before)

A Cold Reader agent (a smaller model, no repo, no memory) read README.md at `ee7dbda` with the brief in [doc-roles.md](../contributing/doc-roles.md).

| Question | Answer from the page | Correct |
|---|---|---|
| 1. What is it? | A terminal tool that draws annotations on code, step by step | yes |
| 2. Who is it for? | People who give walkthroughs, present, record or debug with an LLM; also AI agents. The reader was "unsure": the page has no "for" statement. | yes |
| 3. How do I install it? | Releases or `cargo install tiza`. The reader could not find how to unpack and place the binary. | yes |
| 4. What is the first command to try? | `tiza src/main.rs`. It needs a file of your own. | yes |
| 5. Where are all the `:` commands? | Lines 92-129. The block omits 12 commands (§5), so "all" is not true. | no |
| 6. Does it send my code anywhere? | No. Found at line 225, but only with a search for "network". | yes |
| 7. How do I let an AI agent draw on what I am showing? | `tiza mcp` and `claude mcp add tiza -- tiza mcp` | yes |

Result: six of seven correct answers. But the README does not pass: the first command to try, `tiza src/main.rs`, needs a file of your own.

The first command that needs no file of your own is `tiza examples/ownership.rs.tiza --present` (line 70). It works in the folder of a release archive. A test in a pseudo-terminal opened the demo, `n` went to the next step and `:q!` quit with status 0. It does not work after `install.sh` or `cargo install`, because they do not install `examples/` (decision row 16).

Other measures:

- Prose before the first runnable command: 378 words at `ee7dbda` (§2.2). This is the "before" figure. At `4d66ac0` it is 223 words (§2.3).
- The reader got lost first at line 46, the download paragraph: "The install paragraph turns into maintainer budget details".
- Terms that the page uses before it defines them, or never defines: MCP, sidecar, step, take, presentation mode, ADR, stable geometry, Herdr.
- Expected and not found: a one-line binary install, a statement of who it is for, links near the top, a demo that runs after install.
- Its estimate of the time to a first useful command: 4 to 6 minutes with Rust, 8 to 10 minutes with the binary route.

The full report is kept outside the repo.

## 9. Target tree and budgets

### 9.1 Tree (proposal)

```text
README.md                    landing page: what, who, install, first command, map (budget below)
AGENTS.md                    rules and an index for agents that work on the repo (150 lines or fewer)
ARCHITECTURE.md              code map and invariants (architecture work)
CONTRIBUTING.md  SECURITY.md  CHANGELOG.md   stay at the root
llms.txt                     decision row 8
docs/
  README.md                  hub: "Pick your path", then "I want to..." with one link per row
  SUMMARY.md                 the mdBook table of contents
  getting-started.md         TUTORIAL: ten minutes, with code that the reader copies from the page
  guides/                    HOW-TO: install, present, change-code-between-steps, several-files,
                             follow-the-editor, record-gif-or-video, check-before-you-record,
                             themes, zed (from docs/zed.md)
  reference/                 REFERENCE: keys, commands, cli, document-format, exports,
                             mcp-tools, socket-api, terminals, themes
  concepts/                  EXPLANATION: glossary, anchors, steps-and-moves, stable-geometry,
                             saving, privacy-and-network
  agents/                    for agents that USE Tiza: setup, using-tiza (from agent-guide.md),
                             supervised-mode, scenarios (from demos/scenarios.md)
  contributing/              development, releasing, ui-guidelines, writing-docs, doc-roles
  examples/                  the gallery and one page per feature group, each example inline
  adr/  research/  release-notes/  brand/
  img/                       README images only (decision row 1)
examples/                    the example sources, for the figures and the tests; no user step needs it
qa/README.md                 stays (decision row 9)
```

What changed from the earlier proposal, and why:

- No secret masking guide and no "redaction fails closed" page. Redaction left the product before v0.1.0.
- `concepts/saving.md` is new. The README "Saving" paragraph is 700 words, and half of it is explanation.
- `concepts/glossary.md` is new. It becomes the one home of the definitions, and the writing guide then keeps only the "Use" and "Do not use" columns. The pilot readers of zed.md and "Terminal support" did not know the words "Tiza", "step" and "document" (§12).
- `agents/setup.md` is new. It is for an agent that helps a person install and connect Tiza. Herdr keeps such a page apart from the page for an agent that uses the tool.
- `reference/terminals.md` and `reference/themes.md` are new. Mouse moves into `guides/present.md`.
- `docs/examples/` is new. The owner wants the examples on the site, so no user step needs the repository.
- `qa/README.md` stays in `qa/`. It is the contract of the QA crates, and the tests live there.

### 9.2 The docs site

The site is https://tiza.luisk.dev on GitHub Pages, built with mdBook from the same `docs/` folder (decided).

- mdBook reads `docs/SUMMARY.md`. A page that is not in it is not in the book. The docs checks must fail when a page is missing from it.
- The Markdown stays plain, so GitHub shows the same pages if the site is down.
- The site comes early, in its own work package, with the pages that exist today. It grows when the skeleton, the reference pages and the new README land.
- mdBook is a CI tool, not a dependency of the workspace. The workflow pins its version and checks its SHA-256.
- The site has no analytics, no tracking and no external fonts or scripts.
- DNS is ready. The GitHub Pages settings are the owner's steps. An agent does not do them.
- The site deploys on each push to `main` that changes `docs/**`, `book.toml` or the workflow (decided, row 15). Its header names the latest release.

The pages of today have no relative link that leaves `docs/`, so they work on the site as they are. A first `docs/SUMMARY.md` (proposal):

```text
# Summary

[Tiza](README.md)                      a short new hub page: the positioning sentence,
                                       install (install.sh first), links to the README and the repo

# Use Tiza
- [Tiza from Zed](zed.md)

# Agents
- [Agent guide](agent-guide.md)
- [Demo scenarios](demos/scenarios.md)

# Releases
- [v0.1.0](release-notes/v0.1.0.md)

# Contributing
- [Writing the docs](contributing/writing-docs.md)
- [Doc roles](contributing/doc-roles.md)
- [UI guidelines](ui-guidelines.md)
- [Documentation structure](research/documentation-structure.md)

# Decisions
- [Architecture decision records](adr/README.md)
  - one line for each ADR, 0001 to 0009
```

Out of the first book: the other research notes, `brand/` (assets) and `img/`. This note is in, because the writing guide links to it. The README itself stays out: its relative links to `docs/` would break inside the book. The skeleton work package moves `zed.md`, `agent-guide.md`, `demos/scenarios.md` and `ui-guidelines.md` later, and changes `SUMMARY.md` in the same commit.

### 9.3 Budgets (proposal)

| Item | Budget | Reason |
|---|---|---|
| README prose | 1,000 words or fewer | ripgrep's README is about 3,000 words and links to a guide; Herdr's is about 600; ratatui's prose is about 500 |
| README lines | 150 or fewer | one screen of install and first command, then links |
| README H2 sections | 8 or fewer | a short row of links can name them all |
| Prose before the first command | 150 words or fewer | 378 at v0.1.0, 223 after the install commit |
| A paragraph | five sentences or fewer | the reader can scan it |
| How-to guide | 150 lines or fewer | one task |
| Concept page | 80 lines or fewer | one idea |
| Tutorial | 250 lines or fewer | ten minutes |
| Reference page | no line budget; one row per item | complete is the goal |
| AGENTS.md | 150 lines or fewer | Osmani: "Aim for 150 lines or fewer." |
| `docs/img` | 1 MB or less, without `src/` | with path C it holds the README images only |
| One GIF in the repo | 400 kB or less | the ownership tour at the spec is 385 kB |

### 9.4 Figures

The figure spec is decided. It is in the writing guide, §6.

- Snippet: 53 columns or fewer, `--font-size 13 --scale 2`, `width="466"`.
- Full step: 100 columns, `--font-size 12 --scale 2`, `width="768"`.
- Tour GIF: 100 columns (106 to 130 if needed), `--font-size 12 --scale 2`.
- On a phone a full step shows its text at about 5 px. The figure is a map that opens in full when you tap it. Reference pages add the `md` render in a `<details>` block.

Where the figures live is open (decision row 1). Measured with 29 planned figures:

| | A: all in the repo | B: rendered when the site is built | C: README in the repo, docs at build |
|---|---|---|---|
| First commit | +2.51 MiB | about 20 kB of sources | about 0.41 MiB |
| Each visual change of the renderer | +2.49 MiB | 0 | +0.41 MiB |
| Works with no CI and no site | yes | no | the README, yes |
| Old figures | need a freshness check | not possible | the README check exists today |

Linux x86_64 and macOS arm64 give the same bytes for all 50 test outputs. So a render in CI is the same as a render on the Mac.

Recommendation: C. The README must show its images on GitHub and on crates.io with no site. The docs pages are for the site, and CI builds the site.

### 9.5 Links on crates.io and in the release archive

- crates.io shows `crates/tiza-cli/README.md`, not the root README. Its links are absolute URLs. So crates.io works today. The crate README is one more copy to keep true (§6.1).
- The release archive holds `tiza`, README.md, the two licenses and `examples/`. README.md has 21 relative links to files. 4 work in the archive: the two licenses and the two links into `examples/`. The other 17 break, the three images included.
- The new README must link to pages outside `examples/` with absolute URLs: the site when it exists, GitHub until then. Images use `raw.githubusercontent.com` URLs pinned to the tag, or stay relative and accept that the archive shows no images. The new README work package decides with a test.

### 9.6 Work packages: confirm, resize or drop

| Work package | Verdict | Note |
|---|---|---|
| Docs checks | confirm | Add: `SUMMARY.md` lists every page; one spelling of color; the same positioning sentence everywhere; no ticket id. Use the budgets of §9.3. |
| Docs skeleton | confirm | Add "Pick your path" in the hub. Each new page and each `git mv` changes `docs/SUMMARY.md` in the same commit. `git mv` only for `docs/zed.md`, `docs/ui-guidelines.md`, `docs/demos/scenarios.md` and `docs/agent-guide.md`. |
| Reference pages | confirm, resize | Nine pages (adds terminals and themes). The drift tests of §5 are part of it. |
| Tutorial and guides | resize | Nine guides. No secret masking guide. Each guide starts with one sentence about Tiza and links to install. |
| Concept pages | resize | Six pages: glossary, anchors, steps-and-moves, stable-geometry, saving, privacy-and-network. No redaction page. |
| Figures pipeline | confirm | The spec is decided. The rest depends on decision row 1. |
| Agent docs | confirm, resize | Add `agents/setup.md`. AGENTS.md gets boundaries, an index and the change-to-page map of §10. |
| New README | confirm | Budgets of §9.3. Absolute links (§9.5). `install.sh` is the first install route. It runs the final Cold Reader and Editor round. |
| Examples | confirm | Groups in §11. |
| MCP tool families | confirm | Its tool reference replaces the README tool list. |
| Docs site (new) | add | mdBook, a Pages workflow, `tiza.luisk.dev`. It starts right after this note is approved, with the pages of today (§9.2), in parallel with the skeleton. |

### 9.7 Order of the work packages

| Order | Work package | Starts after |
|---:|---|---|
| 1 | This note: approval by the owner | |
| 2 | Docs site and docs skeleton, in parallel | 1 |
| 3 | Docs checks | 2 |
| 4 | Quick pass (decision row 14), if approved | 2 |
| 5 | Figures pipeline | decision row 1 |
| 6 | Reference pages, concept pages, agent docs | 3 |
| 7 | Tutorial and guides, examples, MCP tool families | 5 and 6 |
| 8 | New README, with the final Cold Reader and Editor round | 7 |

## 10. Change-to-page map

A commit that changes behavior updates the page in the same commit. This map goes into AGENTS.md with the agent docs work package.

| Change | Pages |
|---|---|
| A key | `reference/keys.md`, `keys::HELP` |
| A `:` command | `reference/commands.md`, and the guide that uses it |
| A CLI flag or subcommand | `reference/cli.md` or `reference/exports.md` |
| A document field | `reference/document-format.md`, `schema/tiza.schema.json` |
| An MCP tool or resource | `reference/mcp-tools.md`, `agents/using-tiza.md` if the flow changes |
| The socket protocol | `reference/socket-api.md`, `schema/api.schema.json`, the MCP tools |
| A visual change of the renderer | the README images (`scripts/readme-images.sh`), the docs figures |
| A terminal behavior | `reference/terminals.md` |
| Something that reads or writes a file, or the network | `concepts/privacy-and-network.md`, SECURITY.md |
| A release | CHANGELOG.md, `docs/release-notes/` |

## 11. Examples

Today `examples/` holds five examples, all in Rust: `ownership.rs.tiza`, `ownership-moves.rs.tiza`, `stack-impl.tiza`, `stack-and-queue.tiza` and `tally-tour.tiza`, plus one theme file. `docs/demos/scenarios.md` describes 12 agent scenarios.

Feature groups (proposal, from the earlier plan, checked against the tree):

| Group | Features | Page on the site | Its guide |
|---|---|---|---|
| Annotations | marks and styles, arrows, boxes, highlights, notes, callouts, spotlights | `docs/examples/annotations.md` | `getting-started.md`, `reference/commands.md` |
| Steps and presentation | steps, presentation mode, stable geometry, named steps, live pointer | `docs/examples/steps.md` | `guides/present.md` |
| Moves | insert, replace, delete, typing, cursor, select, wait | `docs/examples/moves.md` | `guides/change-code-between-steps.md` |
| Files and views | `code.link`, ranges, embedded code, follow mode | `docs/examples/views.md` | `guides/several-files.md`, `guides/follow-the-editor.md` |
| Export | text, PNG, GIF, video, cast, tape | the same examples, rendered | `guides/record-gif-or-video.md` |
| Themes | dark, light, terminal, a theme file | `docs/examples/themes.md` | `guides/themes.md` |
| Agents | a `tiza api` script and the same flow as MCP calls | `docs/examples/agents.md` | `agents/using-tiza.md`, `agents/scenarios.md` |

- The secrets group is dropped: redaction left the product.
- The examples are on the site (decision row 16). Each page shows the figure and the code, and the reader copies the `.tiza` from the page.
- The sources stay in `examples/<group>/`. The figures pipeline renders them, and the tests open them. A page includes each source with mdBook `{{#include}}`, so the page and the file cannot differ. The docs site work package checks that this works for a file outside `docs/`.
- The gallery is `docs/examples/README.md`: one row per complete example, with its language, what it teaches and its figure. The Rust ownership example stays first.
- `examples/README.md` becomes a short index for contributors.
- Small examples rotate languages: Rust, Python, TypeScript, SQL and a config file.

## 12. Glossary

The writing guide holds the glossary. These are the conflicts found, with counts from README.md, the agent guide, the CLI help, the MCP `tools/list`, the schemas and the TUI source.

| Concept | Words found | Proposal |
|---|---|---|
| The `.tiza` file | document (README 37, agent guide 40), scene (11, 35), sidecar (6, 2; other docs 18) | "document". "scene" stays in technical names only (`tiza scene`, `scene.get`, `tiza://scene`). Drop "sidecar". |
| Spelling | colour (README 24, agent guide 3, `main.rs` line 613, `theme.rs` 5 places, the `tiza render` summary), color (README 12, agent guide 13, MCP 19) | "color". The JSON field and the command are `color`. |
| One image of an export | frame (README 14), step | "frame" for an image, "step" for the state |
| The whole document shown step by step | take (as a noun: README 17, agent guide 7), presentation | Decision row 5 |
| The person who shows the code | presenter (agent guide 43, MCP 7), you (README) | "you" in user pages; "the presenter" in agent pages |
| The normal screen | authoring screen (README 2), normal mode | "normal mode" |
| A mark and its name | mark, kind, style | three words, three concepts: see the guide |
| A code change at a step | move (also a mark kind and "move the cursor"), patch | "move"; say "the `move` mark" for the kind |
| A file shown with `code.link` | view (README 5, agent guide 16), linked file | "view" for the view, "linked file" for the file |
| The running TUI | instance (README 7, CLI help 25), live instance | "instance" |
| Dimming | spotlight (annotation), focus (`:focus`), live pointer (`f`) | three features; the glossary page defines each |
| The bottom row of normal mode | status line (README 6, TUI source 11), status bar (3, 13), the bar (8, 40) | "status bar" |
| The terminal color feature | truecolor (README 2), 24-bit (7), RGB | "24-bit color" |

## 13. Order with the architecture work

The architecture work also starts with a re-analysis. It owns the AGENTS.md "Index" and "Code rules" headings and makes single sources for the `:` command table and the MCP tool list.

- Option 1: the docs structure goes first (checks, skeleton, concepts, figures, agent docs). The reference pages wait for the two single sources.
- Option 2: the reference pages go first, with drift tests against today's code. The tests change when the sources move.
- Option 3: the architecture work goes first, and the docs wait.

Recommendation: option 1. A drift test can read `App::COMMANDS`, `keys::HELP` and `tools/list` today. The skeleton creates the AGENTS.md index heading if it does not exist yet.

## 14. Pilot of the roles

The Cold Reader and the Editor ran read-only over `docs/zed.md` and the README "Terminal support" section. Each role ran in its own agent. The two Cold Readers used a smaller model and got one file each.

What the Cold Readers found:

- `docs/zed.md`: four of five task questions answered, two of them "unsure". The page never says what Tiza is or how to install it. It has no numbered steps and no success check. The reader estimated 20 to 30 minutes, and 5 minutes for a person who knows Tiza.
- "Terminal support": all five questions answered. The page gives no command to check 24-bit color. It names "Mouse above" and `#themes`, which are not on the page. The tmux item is about 200 words.

What the Editor found (the main items):

- `docs/zed.md` mixes three forms: the steps, the reasons (lines 43 and 69) and a reference list ("Gotchas").
- About 20 sentences break the STE limits, with dash asides, fragments and idioms ("Gotchas", "harmless", "worse").
- Two claims do not match the code. The quoted message "(use --session for another)" is "(use --session for another one)" in `crates/tiza-tui/src/ipc.rs`. A bare `stack.rs` given to `--lang` is not read as a path: only a value with `/` or `\` is (`crates/tiza-cli/src/main.rs`).
- ADR 0007 says the Zed step tasks are "not yet run in Zed", but `docs/zed.md` says they were run.
- 10 facts live in two places, for example Zed `line_height`, language detection and the OSC color query.
- Many sentences that need a style fix are pinned by QA tests ("colours", "reports welcome", "is worse").

What changed in the guide because of the pilot:

- A guide also says what Tiza is and links to install.
- Budgets for a guide, a concept page and the tutorial are in the guide, not only in this note.
- "Five lines" became "five sentences": every paragraph in `docs/zed.md` is one source line.
- New rules: straight quotes, bold labels only before a full sentence, no "above" or "below" across sections, an ADR as a link without its section, "tested with" lines as volatile numbers, and a pinned test changes in the same commit.
- New glossary rows: "24-bit color" and "status bar".
- The roles page got questions and a pass rule for a how-to guide and for a reference page. The Editor now finds the tests that pin a page.

The wrong message text, the `--lang` example and the ADR line are doc errors. The guides work package fixes them. They are not fixed here, because this work changes no existing file.

## 15. Decisions for the owner

| # | Decision | Options | Recommendation |
|---|---|---|---|
| 1 | Where figures live | A (repo), B (site build), C (README in repo, docs at build) | C (§9.4) |
| 2 | Readers in priority order | §3.2, or another order | §3.2 |
| 3 | Target tree | §9.1, or changes | §9.1 |
| 4 | README budget | §9.3, or other numbers | §9.3 |
| 5 | "take" | keep it and define it; replace it with "presentation" in user pages; replace it everywhere | keep "take" in agent pages and the MCP descriptions, define it in the glossary, use "presentation" in user pages |
| 6 | Glossary conflicts | §12 | §12; "color" everywhere |
| 7 | Cold read question 5 | "Where are all the `:` commands?", or "How do I export a GIF?" | keep it: the README fails it today, and the reference pages fix it |
| 8 | `llms.txt` | no; in the repo with URLs pinned to the tag; at the root of the site | at the root of the site, when the site exists |
| 9 | Homes of agent-guide, qa/README, CONTRIBUTING, SECURITY | §4.3 and §9.1 | agent guide to `docs/agents/`; the other three stay |
| 10 | Order with the architecture work | §13 | option 1 |
| 11 | Example groups | §11 | §11 |
| 12 | The hero image | make it again with the new sentence; remove it and keep the logo and the GIF | make it again now: it shows the old sentence. A QA test pins the old sentence in `hero.html`, and another needs the hero on line 1 of the README; both change in the same commit |
| 13 | Home of the full table of tests that read the docs | outside the repo (today); `qa/doc-readers.md`, where test names may appear | `qa/doc-readers.md`, so the work packages that move text can read it |
| 14 | A first quick pass before the full plan | move the README reference verbatim to `docs/reference/` and leave a short README now; or follow the work packages in order | the quick pass: the README is hard to read today, and verbatim moves lose no text |
| 15 | When the docs site deploys | (a) each push to `main` that changes `docs/**`; (b) each release tag `vX.Y.Z` | **Decided by the owner on 2026-10-25: (a).** The site deploys on each push to `main` that changes `docs/**`, `book.toml` or the workflow, and on `workflow_dispatch`. The site header says "Docs for `main`. Latest release: vX.Y.Z" with a link to the release. A page about something not in a release says "Not released yet". The version bump commit removes these marks. Docs per version can come later. |
| 16 | The first command after `install.sh` or `cargo install` | (a) a demo inside the binary; (b) `curl` the example file; (c) open a file of your own; (d) `install.sh` copies `examples/` | **Decided by the owner on 2026-10-25: none of them.** The docs carry the examples, as the Herdr docs carry a quick start: a person does not want to download a repository to try a tool. The quick start works right after `install.sh` or `cargo install`: the reader copies a short code block into a file, or opens a file of their own. The site shows every example with its code. `install.sh` does not change. |

## 16. References

Opened on 2026-10-25 for this note:

- Google, READMEs, https://google.github.io/styleguide/docguide/READMEs.html: "A README is a short summary of the contents of a directory." "At a minimum, your `README.md` file should contain a link to your user- and/or team-facing documentation."
- Diataxis, https://diataxis.fr/start-here/: "A tutorial is a lesson, that takes a student by the hand through a learning experience." "Crossing or blurring the boundaries described in the map is at the heart of a vast number of problems in documentation."
- Addy Osmani, AGENTS.md, https://addyosmani.com/agents/15-agents-md/: "Aim for 150 lines or fewer." Six areas: commands, testing, project structure, code style, git workflow, boundaries.
- ripgrep README, https://github.com/BurntSushi/ripgrep: about 3,000 words; a row of quick links; "Why should I use ripgrep?" and "Why shouldn't I use ripgrep?".
- ratatui README, https://github.com/ratatui/ratatui: about 500 words of prose; concepts and tutorials at https://ratatui.rs, the API at https://docs.rs/ratatui.
- bat README, https://github.com/sharkdp/bat: 12 H2 sections; installation as one H3 per platform with one command each.
- Herdr README, https://github.com/herdrdev/herdr: about 110 lines and 600 words; install with `curl … | sh` first.
- Herdr `llms.txt`, https://herdr.dev/llms.txt: an index of raw source URLs pinned to the release tag.
- mdBook, https://rust-lang.github.io/mdBook/format/summary.html: "The summary file is used by mdBook to know what chapters to include."

The word counts of other projects are estimates from a page summary. Count again before you quote them in public text.
