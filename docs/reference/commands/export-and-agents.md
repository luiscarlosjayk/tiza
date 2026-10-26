# Export and agent commands

Use these commands to export a step and to review the changes of an agent. This page has no frames, because these commands do not change the drawing. The formats are in [Exports](../exports.md).

## `:export`

Writes a step to a text, ANSI, Markdown, SVG, PNG or WebP file.

```
:export [PATH] [step=N] [width=N] [theme=T]
```

<dl>
<dt><code>PATH</code></dt>
<dd>

The file to write. Its extension selects the format.

Default: `NAME-stepN.txt` next to the document. `NAME` is the name of the document without `.tiza`. `N` is the step.

</dd>
<dt><code>step=N</code></dt>
<dd>

The step to export.

Default: the step on screen.

</dd>
<dt><code>width=N</code></dt>
<dd>

The width of the take, from 20 to 400 columns.

Default: the terminal width minus 2, at least 40.

</dd>
<dt><code>theme=T</code></dt>
<dd>

The colors of the export.

`dark` | `light`

Default: the theme on screen.

</dd>
</dl>

For example, `:export step2.png step=2 width=80` writes step 2 to a PNG file, 80 columns wide.

The other rules and the messages of `:export` are in [Exports](../exports.md#export).

## `:supervise`

Starts supervised mode, stops it, or shows its state. In supervised mode, each change of an agent waits for your decision.

```
:supervise on
:supervise off
:supervise
```

- `:supervise` alone also shows the number of changes that wait.
- Supervised mode is in [Supervise an agent](../../guides/supervise-an-agent.md).

## `:review`

Shows the changes of the agents that wait.

```
:review
```

The keys of the review are in [Agent review](../keys.md#agent-review).
