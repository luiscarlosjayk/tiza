# `:` commands

Type `:` in the Tiza window, then one of these commands.

`L` is a line number and `text` an exact fragment to find on that line (`.` is the cursor line).

```
:mark warn 3:s careful here           built-in kinds: info warn error ok question important read write move new drop scope flow todo
:mark 🔥 4:res.json hot path          one-off mark: any glyph/emoji + your legend
:style io glyph=⇄ legend="network I/O" color=cyan   reusable style, then :mark io 2:fetch
:arrow 2:s 15:s: String move          arrow between two points, with a label
:arrow 7:&t 19:s borrow stroke=dashed  arrow styles: stroke=solid|dashed|dotted|thick|double  route=rounded|sharp  heads=START,END (none arrow dot bar open)
:set a2 stroke=thick end=bar          change properties of an annotation (label, color, step; arrows: stroke, route, heads, start, end; boxes/callouts: border, corners, fill)
:box 15-17 takes owns s               box around a range of lines
:box @'// begin'..@'// end' label      range by two text markers (also :callout, :spot); quotes optional without spaces
:box 15-17 takes border=double corners=sharp fill=no   borders: border=solid|dashed|dotted|thick|double  corners=rounded|sharp  fill=yes|no (callouts take the same keys)
:hl 7:&t shared borrow                highlight (L:text must be on line L — a miss says where the text is; L: alone is the whole line)
:hl 7:&t shared emphasis=underline      how the span is drawn: fill (tint, default) · underline (a thin ‾ row under it) · brackets (⌈ ⌉) · strike — the glyphs are drawn in a softened shade of the annotation's colour, so the code reads first
:note 4 s no longer owns it           note anchored to a line
:callout 15-25 @auto title="why" two or three sentences…   bordered paragraph; @margin (default) | @below | @auto
:step 2        :step a3 2             change step · move annotation a3 to step 2
:goto borrow   :goto 3                 jump to a named step (`steps` in the .tiza) or a step number, as if walked there from step 0
:del a1        :clear | :clear 2       delete one · clear everything or only one step (u undoes)
:label a2 new text                     change the label
:color a2 blue   :color a3 #ff8800   color per annotation (orange blue green purple pink cyan yellow red gray · auto)
:spot 15-17 a2 a3   :spot a6 a7      spotlight: in this step everything but those lines/annotations is dimmed
:until s1 4                            spotlight s1 stays active until step 4
:focus 15-17   :focus                 dim everything but a range · clear the focus
:layout stable | :layout dynamic       rehearse with the take's geometry (`⊞ stable` on the bar) · let the authoring screen grow as steps add arrows
:theme light | dark | terminal | auto   :theme file PATH   theme (see «Themes»); `:theme` alone toggles
:lang ts | :lang auto   :pad 1 4   :w   :q      (:pad = blank margins, saved in the document; :pad 0 0 none, :pad default)
:margin 24 | :margin auto | :margin     cap the right margin at N cells (12-200) so long notes wrap earlier on a wide terminal; saved as `margin`; `render --margin-cols N` overrides it once
:numbers off | :numbers on | :numbers   hide or show the line-number gutter (saved in the document as `numbers`; exports and agents follow it; `render --no-numbers` / `--numbers` override it once)
:follow on | :follow off | :follow        reload the code when the source file is saved · state
:autosave off | :autosave on | :autosave  stop writing the sidecar on every change (`--no-save` at start) · resume (writes what is pending) · state
:export shot.txt | :export shot.md | :export shot.png [step=N] [width=N] [theme=light]   export what a step shows (.txt .ansi .md .svg .png)
:w PATH | :saveas PATH | :export PATH   a relative PATH starts at the folder where you started Tiza; ~ is your home folder; Tiza creates missing folders
:supervise on | :supervise off | :review   agent changes wait for your y/n (a = all, d = none); 60 s without a decision = rejected
:mouse on | :mouse off                     click moves the cursor, drag selects (then h/b/a/n…), wheel scrolls; off by default
```
