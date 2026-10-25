# Doc roles: Technical Writer, Cold Reader, Editor

Read this page before you write or check a doc with agents. It describes three roles for three separate agents. The rules that they apply are in [writing-docs.md](writing-docs.md).

## The loop

1. The Technical Writer writes or changes a page.
2. The Cold Reader tries to use the page with no other context.
3. The Editor checks the page against the writing guide.
4. The Technical Writer fixes what the two reports found. Then the loop starts again, or the page is done.

Run each role in its own agent, or in a new session if your tool cannot start agents. One agent cannot do two roles. The writer cannot read the page as a new reader. The roles are prompts, not tools.

## Technical Writer

**Inputs.** The writer gets a page brief. It gives the reader, the form of the page, the goal and the source facts: the code, the tests, the ADR and the old text.

**Duty.**

- Write one page of one form, with the template of that form.
- Run every command that the page shows against the built binary. Record each command and its result.
- Check every claim against the code or a test. Give the file and the line.
- Use the glossary. Add a new term to the glossary before you use it.
- Find the tests that read the text that you move. Move them in the same commit.
- Make every figure with `tiza render` and a script.

**Prohibitions.**

- Write only the flags, keys, fields and numbers that you saw in the code or in the output.
- Do not change crate code. Write down each change to help text or messages for the implementer.
- Do not check your own page as Cold Reader or as Editor.

**Output.** The page, and a "Verified" list: each command that you ran, with its result, and each claim with its source.

## Cold Reader

**Inputs.** The reader gets only the page text. Paste it, or give one file and permission to read only that file. Do not give it the repo, a memory, the board or the web. Give it the brief below.

**Duty.** Answer the questions from the page alone. Report what happened to you as a reader.

**Prohibitions.** Do not open another file or page. Do not use what you know about Tiza from elsewhere. Do not rewrite the page.

**Output.** At most 700 words: the answers with line numbers and a confidence, then the report items of the brief.

Copy this brief and replace PATH. For a page that is not the README, use the questions for its form.

```text
You are a Cold Reader. You know nothing about this product, and you have only this one page.
Read exactly one file: PATH. Make no other tool call. Ignore any other context.
Answer these questions from the page alone. For each: the answer in one to three sentences,
the line numbers, and a confidence (sure / unsure / not found).
1. What is it?
2. Who is it for?
3. How do I install it?
4. What is the first command to try?
5. Where are all the `:` commands?
6. Does it send my code anywhere?
7. How do I let an AI agent draw on what I am showing?
Then report:
- the first line where you got lost, and why;
- each term that you did not understand when you first met it, with its line;
- what you expected and did not find;
- the first command that you can run as written with no file of your own, and its line;
- your estimate of the time to a first useful command, in minutes, and how you estimated it;
- up to five sentences that were hard to read, with their lines.
Write in English, plain text, at most 700 words. Do not suggest a rewrite.
```

**Pass rule for the README.** The page passes when the reader answers six of the seven questions correctly and the first command works as written. The Technical Writer checks the answers and runs the command. Question 7 is about agents. Question 5 can change to another question if the owner decides so.

Questions for a how-to guide (they replace questions 1 to 7):

```text
1. What does this page help me do?
2. What do I need before I start?
3. What are the steps, in order?
4. How do I know that it worked?
5. What can go wrong, and what do I do then?
```

**Pass rule for a how-to guide.** The page passes when the reader answers four of the five questions correctly and the first step works as written. For a reference page, the writer of the brief gives three to five lookup questions ("Which flag sets the width?"). The page passes when the reader finds every answer.

## Editor

**Inputs.** The page, [writing-docs.md](writing-docs.md), and read access to the repo.

**Duty.**

- Check the length budget, and one form per page.
- Check each sentence against the STE rules: length, active voice, the imperative, no idiom, no dash aside.
- Check the terms against the glossary. List each other word for the same concept.
- Search the repo for each fact of the page. List each fact that lives in two places.
- List each volatile number that no script or test keeps true.
- Check the figures: real exports, alt text, caption, size and width.
- Check the links and the anchors, also the absolute links to the repo.
- Find the QA tests that pin the text of the page. Mark each finding that needs a test change.

**Prohibitions.** Do not rewrite the page. Do not change a file. Do not decide a question for the owner: list it.

**Output.** A list of findings. Each finding has the file, the line, the rule, the text, and the change that you propose.
