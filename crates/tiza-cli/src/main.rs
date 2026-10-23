//! `tiza`: for humans and agents. Without a subcommand it opens the TUI;
//! `render` draws without opening the TUI; `mcp` starts the MCP server;
//! `scene`/`api` talk to the live instance. JSON by default for machine
//! output, `-` for stdin/stdout, notices on stderr and exit 1 on error.

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand, ValueEnum};
use tiza_core::fs;
use tiza_core::highlight::{has_grammar, normalize_language};
use tiza_core::layout::{LayoutOptions, layout};
use tiza_core::socket;
use tiza_core::{Theme, ThemeName};

#[derive(Parser)]
#[command(name = "tiza", version = tiza_core::VERSION_LONG, about = "Draw on top of code in the terminal to explain it on video.", long_about = None)]
#[command(
    after_help = "Examples:\n  tiza src/main.rs:12-48        open a range of lines\n  pbpaste | tiza --lang rs       code from the clipboard\n  tiza demo.tiza --present   open in presentation mode\n  tiza render demo.tiza --out - --step 2\n  tiza render demo.tiza --out demo.png --step 3   PNG (or .svg) of a step\n  tiza render --out shot.png   PNG of what the live instance shows\n  tiza check demo.tiza           preflight before a take (anchors, moves, clipped labels)\n  tiza render demo.tiza --all-steps --out steps/   one PNG per step\n  tiza mcp                       MCP server for agents\n  tiza next | tiza goto borrow   drive the live instance from a script or an editor task\n  tiza schema api                JSON Schema of the local API (schema document = the .tiza format)\n  cat demo.tiza | tiza render - --out -   `-` is stdin everywhere"
)]
struct Cli {
    /// Source file (`main.rs`, `main.rs:12-48`), `.tiza` document, or `-` for stdin (code or a document). Without it, reads stdin when piped.
    file: Option<String>,
    /// Language override (rs, ts, py, go…); otherwise detected from the extension, file name, shebang or modeline
    #[arg(long)]
    lang: Option<String>,
    /// Draw one frame and exit without touching the document (CI smoke test)
    #[arg(long, hide = true)]
    smoke: bool,
    /// Start with mouse capture on: click moves the cursor, drag selects, wheel scrolls (`:mouse on|off` live).
    /// Off by default because it takes the terminal's own text selection (Shift/Option-drag keeps it).
    #[arg(long)]
    mouse: bool,
    /// Initial presentation step
    #[arg(long)]
    step: Option<u32>,
    /// Theme: dark, light, terminal (the host's own palette) or auto (follow the terminal, the default when the document names none)
    #[arg(long, value_enum)]
    theme: Option<ThemeArg>,
    /// Per-token theme overrides from a TOML file (`[theme]` table or bare tokens), applied as the document's; see examples/themes/
    #[arg(long)]
    theme_file: Option<PathBuf>,
    /// Start in presentation mode (no UI)
    #[arg(long)]
    present: bool,
    /// Focus a range of lines on open (A-B)
    #[arg(long)]
    focus: Option<String>,
    /// Blank cells around the drawing for this session: `V` or `V,H` (`:pad` saves it in the document)
    #[arg(long)]
    padding: Option<String>,
    /// Reload the code when the source file is saved (Zed, any editor); annotations are re-anchored by text
    #[arg(long)]
    follow: bool,
    /// Path of the local API socket (defaults to $TIZA_SOCKET or the session)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    /// Session name: allows several instances (defaults to "default")
    #[arg(long, global = true)]
    session: Option<String>,
    /// Do not expose the local API
    #[arg(long)]
    no_socket: bool,
    /// Never write the sidecar on its own (annotate an example without touching it); `:w` saves explicitly, `:autosave on` resumes
    #[arg(long)]
    no_save: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Clone, Copy, ValueEnum, PartialEq)]
enum ThemeArg {
    Dark,
    Light,
    /// The host terminal's own palette (TUI only; exports fall back to dark/light)
    Terminal,
    /// Follow the terminal's background: light or dark by luminance (TUI only)
    Auto,
}

impl ThemeArg {
    /// The fixed theme this flag names (`auto` names none).
    fn fixed(self) -> Option<ThemeName> {
        match self {
            ThemeArg::Dark => Some(ThemeName::Dark),
            ThemeArg::Light => Some(ThemeName::Light),
            ThemeArg::Terminal => Some(ThemeName::Terminal),
            ThemeArg::Auto => None,
        }
    }
}

/// The `[theme]` table of `~/.config/tiza/config.toml`, with a one-line
/// warning on stderr when the file does not parse (never fatal).
fn config_theme() -> Option<tiza_core::theme::ThemeOverrides> {
    match tiza_core::theme::load_config() {
        Ok(Some((cfg, _))) => cfg.theme,
        Ok(None) => None,
        Err(e) => {
            let e = e.to_string();
            eprintln!(
                "tiza: config ignored: {}",
                tiza_core::printable::printable_text(&e)
            );
            None
        }
    }
}

/// The theme an export paints: the flag or the document's, with the
/// override tables applied; `terminal` and `reset` tokens fall back with a
/// notice on stderr (there is no host palette in a file).
fn export_theme(
    flag: Option<ThemeArg>,
    doc: &tiza_core::Document,
    theme_file: Option<&PathBuf>,
) -> anyhow::Result<Theme> {
    let name = flag
        .and_then(ThemeArg::fixed)
        .or(doc.theme)
        .unwrap_or(ThemeName::Dark);
    let file = match theme_file {
        Some(p) => Some(tiza_core::theme::load_theme_file(p).map_err(|e| anyhow::anyhow!("{e}"))?),
        None => None,
    };
    let (theme, notes) = Theme::resolve(
        name,
        true,
        config_theme().as_ref(),
        file.as_ref().or(doc.theme_overrides.as_ref()),
    );
    // The notes quote the overrides as written, from the document or a
    // theme file: shown, never sent raw to the terminal.
    for n in notes {
        eprintln!("tiza: {}", tiza_core::printable::printable_text(&n));
    }
    let (theme, notice) = theme.for_export();
    if let Some(n) = notice {
        eprintln!("tiza: {}", tiza_core::printable::printable_text(&n));
    }
    Ok(theme)
}

#[derive(Clone, Copy, ValueEnum, PartialEq)]
enum Format {
    Txt,
    Ansi,
    /// Plain text inside a fenced code block (README, issues, chat)
    Md,
    Svg,
    Png,
    /// asciicast v3 recording of every step (asciinema, agg → GIF)
    Cast,
    /// VHS tape that drives the real binary through every step (vhs → GIF/MP4)
    Tape,
    /// Animated GIF of every step, drawn by Tiza itself (nothing to install)
    Gif,
    /// MP4 of every step through `ffmpeg` on your PATH (never bundled)
    Mp4,
    /// WebM of every step through `ffmpeg` on your PATH (never bundled)
    Webm,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render a document without opening the TUI: text, ANSI, SVG or PNG.
    /// Without FILE it exports what the live instance is showing.
    Render {
        /// `.tiza` document (or source file) to draw; omit it to use the live instance
        file: Option<String>,
        /// Output: a path or `-` for stdout. The extension picks the format (`.txt`, `.ansi`, `.md`, `.svg`, `.png`, `.gif`, `.mp4`, `.webm`, `.cast`, `.tape`) unless --format is given.
        #[arg(long, default_value = "-")]
        out: String,
        /// Step to show (default: the document's current step, or all of them for a file)
        #[arg(long)]
        step: Option<u32>,
        /// One image per step (step 0 included), all the same size: `--out DIR/` gets `step-00.png` … `step-NN.png` (`--format svg|txt|ansi|md` for others)
        #[arg(long, conflicts_with = "step")]
        all_steps: bool,
        /// Width in columns (default: 100, or the live terminal's width)
        #[arg(long)]
        width: Option<usize>,
        #[arg(long, value_enum)]
        format: Option<Format>,
        /// dark or light (terminal/auto have no palette outside the terminal: dark is used)
        #[arg(long, value_enum)]
        theme: Option<ThemeArg>,
        /// Per-token theme overrides from a TOML file for this export (instead of the document's)
        #[arg(long)]
        theme_file: Option<PathBuf>,
        /// No line-number gutter (overrides the document's `numbers` field for this export)
        #[arg(long, conflicts_with = "numbers")]
        no_numbers: bool,
        /// Force the line-number gutter on (overrides a document saved with `:numbers off`)
        #[arg(long)]
        numbers: bool,
        /// Cap on the right margin in cells (notes wrap there; overrides the document's `margin` field)
        #[arg(long)]
        margin_cols: Option<usize>,
        /// Language override (otherwise detected from the file or its content)
        #[arg(long)]
        lang: Option<String>,
        /// SVG/PNG: font size in px (cells are 0.6 × 1.32 em)
        #[arg(long, default_value_t = 16.0)]
        font_size: f32,
        /// SVG/PNG: blank margin around the drawing, in cells
        #[arg(long, default_value_t = 1.5)]
        margin: f32,
        /// SVG/PNG: corner radius of the background in px (0 = square)
        #[arg(long, default_value_t = 12.0)]
        radius: f32,
        /// PNG/GIF/MP4: device scale (default 2 for PNG and GIF, 1 for MP4/WebM)
        #[arg(long)]
        scale: Option<f32>,
        /// SVG: do not embed the fonts (smaller file; relies on the viewer's fonts)
        #[arg(long)]
        no_embed_fonts: bool,
        /// PNG: also use the system fonts as a fallback (emoji, CJK). Off by default so the output is reproducible.
        #[arg(long)]
        system_fonts: bool,
        /// .gif/.mp4/.cast/.tape: seconds each step stays on screen (the last one twice as long). In a .gif/.mp4/.webm (and the real TUI a .tape drives) a named step with `auto_advance_ms` stays that long instead
        #[arg(long, default_value_t = 2.0)]
        interval: f64,
        /// .gif/.mp4/.cast/.tape and --all-steps: start at step 1 instead of step 0 (the bare code)
        #[arg(long)]
        skip_zero: bool,
        /// .cast/.tape: recording title (default: the document's)
        #[arg(long)]
        title: Option<String>,
        /// .gif: how many times it plays (0 = forever, the default)
        #[arg(long = "loop", default_value_t = 0)]
        loop_count: u16,
    },
    /// Preflight before a take: load a .tiza and list every problem (anchors, markers, moves at their
    /// step, linked files, ids, annotations a move hides, labels
    /// clipped with «…», steps with nothing new). Exit 1 on errors; warnings fail only with --strict.
    /// Writes nothing.
    Check {
        /// `.tiza` document, a source file with a `.tiza` sidecar, or `-` for stdin
        file: String,
        /// Width to check the labels at (default: 100, as `render`)
        #[arg(long)]
        width: Option<usize>,
        /// Print the findings as JSON (for agents and scripts)
        #[arg(long)]
        json: bool,
        /// Warnings fail too (exit 1)
        #[arg(long)]
        strict: bool,
        /// `check` never talks to the live instance: the global `--socket`
        /// and `--session` are accepted (and ignored) but left out of its help.
        #[arg(long, hide = true)]
        socket: Option<PathBuf>,
        #[arg(long, hide = true)]
        session: Option<String>,
    },
    /// Start the MCP server (stdio) that controls the live instance
    Mcp,
    /// Read or change the scene of the live instance
    Scene {
        #[command(subcommand)]
        cmd: SceneCmd,
    },
    /// Go to the next step of the live instance (step 0 after the last one when the document loops); prints `step N/M · title`
    Next,
    /// Go to the previous step of the live instance
    Prev,
    /// Jump the live instance to a named step (`steps` in the .tiza) or a step number, as if walked there from step 0
    Goto {
        /// The `id` of a named step, or a step number
        id: String,
    },
    /// Call a raw local API method: `tiza api status`, `tiza api annotation.add '{...}'`
    Api {
        method: String,
        /// JSON parameters (or `-` to read them from stdin)
        params: Option<String>,
    },
    /// Print a JSON Schema: `document` (the .tiza format) or `api` (every socket method)
    Schema {
        #[arg(value_enum, default_value_t = SchemaKind::Document)]
        kind: SchemaKind,
        /// Output: a path or `-` for stdout
        #[arg(long, default_value = "-")]
        out: String,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum SchemaKind {
    Document,
    Api,
}

#[derive(Subcommand)]
enum SceneCmd {
    /// Print the full document (JSON)
    Get,
    /// List the annotations
    List,
    /// Show what is on screen as text
    Text {
        /// Columns of the take. Without it, the width of the screen's code
        /// area, as the agents' `visible` judges it (and `check` at that width).
        #[arg(long)]
        width: Option<u32>,
    },
    /// Instance status
    Status,
    /// Replace the live document with a .tiza file or a `scene get`
    /// envelope (`-` = stdin); code references are resolved relative to the
    /// file and sent embedded
    Put {
        file: String,
        /// Show a file that is not a document as code (a source file, a
        /// snippet) instead of refusing it
        #[arg(long)]
        as_code: bool,
    },
    /// Partial change: JSON (`-` = stdin, `@file` = a file) with step, theme,
    /// padding, title, styles and/or annotations merged by id
    Patch { json: String },
}

fn main() {
    if let Err(e) = real_main() {
        // An error can quote the document or a find text: its control
        // characters are shown, never sent to the terminal.
        let e = format!("{e:#}");
        eprintln!("tiza: {}", tiza_core::printable::printable_text(&e));
        std::process::exit(1);
    }
}

fn real_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let socket_path = socket::resolve(cli.socket.as_deref(), cli.session.as_deref());

    match cli.cmd {
        None => run_tui(cli, socket_path),
        Some(Cmd::Render {
            file,
            out,
            step,
            all_steps,
            width,
            format,
            theme,
            theme_file,
            no_numbers,
            numbers,
            margin_cols,
            lang,
            font_size,
            margin,
            radius,
            scale,
            no_embed_fonts,
            system_fonts,
            interval,
            skip_zero,
            title,
            loop_count,
        }) => {
            if all_steps && out == "-" {
                bail!("--all-steps writes one file per step: pass a directory, --out steps/");
            }
            let format = format
                .or(all_steps.then_some(Format::Png))
                .unwrap_or_else(|| {
                    match std::path::Path::new(&out)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref()
                    {
                        Some("svg") => Format::Svg,
                        Some("png") => Format::Png,
                        Some("ansi") | Some("ans") => Format::Ansi,
                        Some("md") | Some("markdown") => Format::Md,
                        Some("cast") => Format::Cast,
                        Some("tape") => Format::Tape,
                        Some("gif") => Format::Gif,
                        Some("mp4") | Some("m4v") | Some("mov") => Format::Mp4,
                        Some("webm") => Format::Webm,
                        _ => Format::Txt,
                    }
                });
            // The document: a file, or the live instance's scene.
            // The live instance's `.tiza` and whether it autosaves: what a
            // `.tape` of it opens.
            let mut live_document: Option<(Option<String>, bool)> = None;
            let asked_step = step;
            let (doc, mut step, width) = match &file {
                Some(f) => {
                    let mut opened = if f == "-" {
                        let mut text = String::new();
                        std::io::stdin().read_to_string(&mut text)?;
                        fs::from_stdin(&text, None)
                            .context("could not read the document from stdin")?
                    } else {
                        fs::open(f).with_context(|| format!("could not open {f}"))?
                    };
                    if let Some(n) = &opened.notice {
                        eprintln!("tiza: {}", tiza_core::printable::printable_text(n));
                    }
                    override_language(
                        &mut opened.document,
                        lang.as_deref().or(cli.lang.as_deref()),
                    );
                    (
                        opened.document,
                        step.unwrap_or(u32::MAX),
                        width.unwrap_or(100),
                    )
                }
                None => {
                    let mut scene: tiza_core::protocol::Scene = serde_json::from_value(call(
                        &socket_path,
                        "scene.get",
                        serde_json::json!({}),
                    )?)
                    .context("unexpected scene.get response")?;
                    // Its linked files too, as the instance shows them:
                    // opening the `.tiza` loads them the same way.
                    // What the screen skips and hides, the export does too.
                    tiza_core::stale::apply_marks(&mut scene.document, &scene.stale, &scene.held);
                    fs::load_scene(&mut scene.document).map_err(|e| {
                        anyhow::anyhow!("the live document cannot be rendered: {e}")
                    })?;
                    let status: tiza_core::protocol::Status = serde_json::from_value(call(
                        &socket_path,
                        "status",
                        serde_json::json!({}),
                    )?)
                    .context("unexpected status response")?;
                    let live_w = (status.cols as usize).saturating_sub(2).max(40);
                    live_document = Some((status.document.clone(), status.autosave));
                    (
                        scene.document,
                        step.unwrap_or(scene.step),
                        width.unwrap_or(live_w),
                    )
                }
            };
            // The gutter: the document's `numbers` field unless a
            // flag forces it for this export.
            let line_numbers = if no_numbers {
                false
            } else if numbers {
                true
            } else {
                doc.numbers
            };
            // Sizes from the command line are bounded: a typo must be
            // an error, never an aborted process.
            let width =
                tiza_core::layout::check_width(width).map_err(|e| anyhow::anyhow!("{e}"))?;
            let margin_cap = match margin_cols {
                Some(c) => Some(
                    tiza_core::layout::check_margin_cap(c).map_err(|e| anyhow::anyhow!("{e}"))?,
                ),
                None => doc.margin.map(usize::from),
            };
            // A hold past one hour (or not a number) is refused:
            // `1e308` overflowed and `1e7` hung the GIF encoder.
            let interval =
                tiza_core::anim::check_interval(interval).map_err(|e| anyhow::anyhow!("{e}"))?;
            let animated = matches!(format, Format::Gif | Format::Mp4 | Format::Webm);
            // Scale 2 for stills and the GIF (long italic notes are soft
            // at 1); video stays at 1, the player scales it.
            let video = matches!(format, Format::Mp4 | Format::Webm);
            let scale = scale.unwrap_or(if video { 1.0 } else { 2.0 });
            if matches!(format, Format::Png | Format::Svg) || animated {
                tiza_core::layout::check_font_size(font_size)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                if format != Format::Svg {
                    tiza_core::layout::check_scale(scale).map_err(|e| anyhow::anyhow!("{e}"))?;
                }
            }
            if animated && out == "-" {
                bail!(
                    "a .{0} needs a file: pass --out demo.{0}",
                    format_ext(format)
                );
            }
            // An explicit step past the last one is probably a typo in a
            // script: say so (on stderr, so `--out -` stays clean) and show
            // the last step, the same take as `--step L`. (Without
            // `--step`, a file shows every step's annotations; a recording
            // shows every step and takes no `--step`.)
            let recording = matches!(
                format,
                Format::Gif | Format::Mp4 | Format::Webm | Format::Cast | Format::Tape
            );
            if let Some(asked) = asked_step
                && asked > doc.max_step()
                && !recording
            {
                eprintln!(
                    "tiza: step {asked} does not exist (the document has steps 0 to {}); showing step {}",
                    doc.max_step(),
                    doc.max_step()
                );
                step = doc.max_step();
            }
            // A `steps` entry past the last step is kept, never dropped in
            // silence.
            for w in doc.step_warnings() {
                eprintln!("tiza: {}", tiza_core::printable::printable_text(&w));
            }
            // A `rebind` entry whose annotation is gone is drawn as if it
            // were not there: said once per entry.
            for r in tiza_core::moves::ignored_rebinds(&doc) {
                let line = format!("{}: {}", r.place(), r.message());
                eprintln!("tiza: {}", tiza_core::printable::printable_text(&line));
            }
            let theme = export_theme(theme, &doc, theme_file.as_ref())?;
            if all_steps {
                let opts = LayoutOptions {
                    width,
                    step: 0,
                    focus: None,
                    pointer: None,
                    line_numbers,
                    tab_width: 4,
                    typing: false,
                    stable: true,
                    margin_cap,
                };
                let svg_opts = tiza_export::SvgOptions {
                    font_size,
                    margin_cells: margin,
                    radius,
                    embed_fonts: format == Format::Svg && !no_embed_fonts,
                    title: doc.title.clone(),
                };
                let png_opts = tiza_export::PngOptions {
                    scale,
                    system_fonts,
                };
                return render_all_steps(
                    &doc, &theme, &out, format, &opts, &svg_opts, &png_opts, skip_zero,
                );
            }
            let l = layout(
                &doc,
                &theme,
                &LayoutOptions {
                    width,
                    step,
                    focus: None,
                    pointer: None,
                    line_numbers,
                    tab_width: 4,
                    typing: false,
                    stable: true,
                    margin_cap,
                },
            );
            let bytes: Vec<u8> = match format {
                Format::Txt => (l.canvas.to_plain_text() + "\n").into_bytes(),
                Format::Ansi => l.canvas.to_ansi().into_bytes(),
                Format::Md => l.canvas.to_markdown(doc.title.as_deref()).into_bytes(),
                Format::Gif | Format::Mp4 | Format::Webm => {
                    let anim_opts = tiza_core::anim::AnimOptions {
                        interval,
                        width,
                        line_numbers,
                        from_zero: !skip_zero,
                    };
                    let frames = tiza_core::anim::frames(&doc, &theme, &anim_opts);
                    // Which steps the document timed itself.
                    let timed = tiza_core::anim::timed_steps(&doc, &anim_opts);
                    let timed_note = match timed.as_slice() {
                        [] => String::new(),
                        [one] => format!(", step {one} timed by its auto_advance_ms"),
                        many => format!(
                            ", steps {} timed by their auto_advance_ms",
                            many.iter()
                                .map(|s| s.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    };
                    let svg_opts = tiza_export::SvgOptions {
                        font_size,
                        margin_cells: margin,
                        radius,
                        embed_fonts: false,
                        title: None,
                    };
                    let png_opts = tiza_export::PngOptions {
                        scale,
                        system_fonts,
                    };
                    if format == Format::Gif {
                        let gif_frames: Vec<(tiza_core::canvas::Canvas, u64)> =
                            frames.into_iter().map(|f| (f.canvas, f.delay_ms)).collect();
                        let (bytes, report) = tiza_export::to_gif(
                            &gif_frames,
                            &theme,
                            &tiza_export::GifOptions {
                                svg: svg_opts,
                                png: png_opts,
                                repeat: (loop_count > 0).then(|| loop_count.saturating_sub(1)),
                            },
                        )
                        .context("could not encode the GIF")?;
                        write_atomically(&out, &bytes)?;
                        eprintln!(
                            "wrote {out} ({} bytes): {} frames, {}×{} px, {:.1} s per loop, {} colours{}{timed_note}",
                            bytes.len(),
                            report.frames,
                            report.width,
                            report.height,
                            report.duration_ms as f64 / 1000.0,
                            report.colors,
                            if loop_count == 0 {
                                ", loops forever".to_string()
                            } else {
                                format!(", plays {loop_count}×")
                            }
                        );
                        return Ok(());
                    }
                    let bytes = encode_video(&frames, &theme, &svg_opts, &png_opts, format, &out)?;
                    eprintln!("wrote {out} ({bytes} bytes){timed_note}");
                    return Ok(());
                }
                Format::Cast | Format::Tape => {
                    let opts = tiza_core::cast::CastOptions {
                        interval,
                        width,
                        rows: None,
                        line_numbers,
                        title: title.clone(),
                        from_zero: !skip_zero,
                    };
                    if format == Format::Cast {
                        tiza_core::cast::to_asciicast(&doc, &theme, &opts).into_bytes()
                    } else {
                        let mut tape_note: Option<String> = None;
                        let document_path = match (&file, &live_document) {
                            (Some(f), _) => f.clone(),
                            // The instance's own `.tiza` (the tape typed
                            // `tiza scene.tiza`, a file that does not exist).
                            (None, Some((Some(d), autosave))) => {
                                // Relative to the tape's folder, where `vhs`
                                // runs it, never the absolute path with the
                                // home in it.
                                let base = if out == "-" {
                                    std::env::current_dir()?
                                } else {
                                    let o = std::path::absolute(&out)?;
                                    o.parent().map(Path::to_path_buf).unwrap_or(o)
                                };
                                let rel = relative_to(Path::new(d), &base)
                                    .to_string_lossy()
                                    .into_owned();
                                if !autosave {
                                    eprintln!(
                                        "tiza: the tape opens {rel}, which this instance does not autosave: save it (:w) first so it holds what is on screen"
                                    );
                                }
                                tape_note = Some(format!(
                                    "# Opens the live instance's document as {rel}, relative to this tape's folder: run vhs from there"
                                ));
                                rel
                            }
                            (None, _) => bail!(
                                "the live document has no .tiza on disk yet for the tape to open: save it in Tiza (:w) or render a .tiza file"
                            ),
                        };
                        let gif = if out == "-" {
                            "demo.gif".to_string()
                        } else {
                            std::path::Path::new(&out)
                                .with_extension("gif")
                                .to_string_lossy()
                                .into_owned()
                        };
                        // VHS reads the tape line by line and types what
                        // it quotes: a path with a newline, a control
                        // character or a `"` would add commands of its
                        // own. Refused, never rewritten: the tape would
                        // open another file.
                        for (what, p) in [("document", &document_path), ("output", &out)] {
                            if !tiza_core::cast::tape_safe_path(p) {
                                bail!(
                                    "cannot write a .tape for the {what} path «{}»: it holds a control character, a newline or a `\"`, which VHS would read as commands of its own; rename the file",
                                    tiza_core::printable::printable_line(p)
                                );
                            }
                        }
                        let mut tape =
                            tiza_core::cast::to_vhs_tape(&doc, &theme, &document_path, &gif, &opts);
                        if let Some(note) = tape_note
                            && let Some(nl) = tape.find('\n')
                        {
                            tape.insert_str(nl + 1, &format!("{note}\n"));
                        }
                        tape.into_bytes()
                    }
                }
                Format::Svg | Format::Png => {
                    let svg_opts = tiza_export::SvgOptions {
                        font_size,
                        margin_cells: margin,
                        radius,
                        embed_fonts: format == Format::Svg && !no_embed_fonts,
                        title: doc.title.clone(),
                    };
                    let svg = tiza_export::to_svg(&l.canvas, &theme, &svg_opts);
                    if format == Format::Svg {
                        svg.into_bytes()
                    } else {
                        tiza_export::to_png(
                            &svg,
                            &tiza_export::PngOptions {
                                scale,
                                system_fonts,
                            },
                        )
                        .context("could not rasterize the PNG")?
                    }
                }
            };
            if out == "-" {
                if format == Format::Png && std::io::stdout().is_terminal() {
                    bail!("refusing to write PNG bytes to the terminal; pass --out file.png");
                }
                std::io::stdout().write_all(&bytes)?;
            } else {
                std::fs::write(&out, &bytes).with_context(|| format!("could not write {out}"))?;
                let shown = tiza_core::printable::printable_text(&out);
                eprintln!("wrote {shown} ({} bytes)", bytes.len());
                match format {
                    Format::Tape => eprintln!(
                        "play it with `vhs {shown}` (needs vhs, ttyd, ffmpeg); `--out {}.gif` needs nothing",
                        tiza_core::printable::printable_text(
                            &std::path::Path::new(&out)
                                .with_extension("")
                                .display()
                                .to_string()
                        )
                    ),
                    Format::Cast => eprintln!(
                        "play it with `asciinema play {shown}` (GIF: `agg {shown} {}`)",
                        tiza_core::printable::printable_text(
                            &std::path::Path::new(&out)
                                .with_extension("gif")
                                .display()
                                .to_string()
                        )
                    ),
                    _ => {}
                }
            }
            Ok(())
        }
        Some(cmd @ (Cmd::Next | Cmd::Prev | Cmd::Goto { .. })) => {
            // Automation for scripts and editor tasks: the same
            // socket as `tiza api`, no JSON to quote; no instance → exit 1.
            let (method, params) = match cmd {
                Cmd::Next => ("step.next", serde_json::json!({})),
                Cmd::Prev => ("step.prev", serde_json::json!({})),
                Cmd::Goto { id } => ("step.goto", serde_json::json!({"id": id})),
                _ => unreachable!(),
            };
            let resp = call(&socket_path, method, params)?;
            let label = resp
                .get("label")
                .and_then(|l| l.as_str())
                .map(String::from)
                .unwrap_or_else(|| {
                    format!(
                        "step {}/{}",
                        resp.get("step").and_then(|v| v.as_u64()).unwrap_or(0),
                        resp.get("max_step").and_then(|v| v.as_u64()).unwrap_or(0)
                    )
                });
            println!("{}", tiza_core::printable::printable_line(&label));
            Ok(())
        }
        Some(Cmd::Check {
            file,
            width,
            json,
            strict,
            ..
        }) => {
            let width = width
                .map(tiza_core::layout::check_width)
                .transpose()
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let opts = tiza_core::check::CheckOptions { width };
            let (name, report) = if file == "-" {
                let mut text = String::new();
                std::io::stdin().read_to_string(&mut text)?;
                let cwd = std::env::current_dir().ok();
                (
                    "stdin".to_string(),
                    tiza_core::check::check_text(&text, cwd.as_deref(), &opts),
                )
            } else {
                let path = std::path::PathBuf::from(&file);
                let path = if fs::is_document_path(&path) {
                    path
                } else {
                    let sidecar = fs::sidecar_path(&path);
                    if !sidecar.exists() {
                        bail!(
                            "{file} is not a .tiza document and has no sidecar ({}): nothing to check",
                            sidecar.display()
                        );
                    }
                    sidecar
                };
                let report = tiza_core::check::check_path(&path, &opts)
                    .with_context(|| format!("could not read {}", path.display()))?;
                (path.display().to_string(), report)
            };
            let failed = !report.ok || (strict && report.warnings > 0);
            let mut stdout = std::io::stdout();
            if json {
                writeln!(
                    stdout,
                    "{}",
                    json_text(serde_json::to_string_pretty(&report)?)
                )?;
            } else {
                let text = report.to_text(&name);
                write!(stdout, "{}", tiza_core::printable::printable_text(&text))?;
            }
            stdout.flush()?;
            if failed {
                std::process::exit(1);
            }
            Ok(())
        }
        Some(Cmd::Mcp) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(tiza_mcp::serve(socket_path))
        }
        Some(Cmd::Scene { cmd }) => {
            let (method, params) = match cmd {
                SceneCmd::Get => ("scene.get", serde_json::json!({})),
                SceneCmd::List => ("scene.list", serde_json::json!({})),
                SceneCmd::Text { width } => ("render.text", serde_json::json!({"width": width})),
                SceneCmd::Status => ("status", serde_json::json!({})),
                SceneCmd::Put { file, as_code } => {
                    // On the wire the code travels embedded (the instance may
                    // run in another directory), linked files included, as
                    // `scene.get` hands it out.
                    let document = document_for_put(&file, as_code)?.embedded_copy();
                    ("scene.put", serde_json::json!({"document": document}))
                }
                SceneCmd::Patch { json } => {
                    let text = if json == "-" {
                        let mut s = String::new();
                        std::io::stdin().read_to_string(&mut s)?;
                        s
                    } else if let Some(path) = json.strip_prefix('@') {
                        std::fs::read_to_string(path)
                            .with_context(|| format!("could not read {path}"))?
                    } else {
                        json
                    };
                    let params: serde_json::Value =
                        serde_json::from_str(&text).context("the patch is not valid JSON")?;
                    ("scene.patch", params)
                }
            };
            let resp = call(&socket_path, method, params)?;
            match method {
                "render.text" => println!(
                    "{}",
                    tiza_core::printable::printable_text(
                        resp.get("text").and_then(|t| t.as_str()).unwrap_or("")
                    )
                ),
                "scene.list" => {
                    for l in resp.as_array().into_iter().flatten() {
                        println!(
                            "{}",
                            tiza_core::printable::printable_line(l.as_str().unwrap_or(""))
                        );
                    }
                }
                _ => println!("{}", json_text(serde_json::to_string_pretty(&resp)?)),
            }
            Ok(())
        }
        Some(Cmd::Schema { kind, out }) => {
            let value = match kind {
                SchemaKind::Document => tiza_core::schema::document_schema(),
                SchemaKind::Api => serde_json::to_value(tiza_core::schema::api_schema())?,
            };
            let text = serde_json::to_string_pretty(&value)? + "\n";
            if out == "-" {
                std::io::stdout().write_all(text.as_bytes())?;
            } else {
                std::fs::write(&out, text).with_context(|| format!("could not write {out}"))?;
                eprintln!("wrote {}", tiza_core::printable::printable_text(&out));
            }
            Ok(())
        }
        Some(Cmd::Api { method, params }) => {
            let params = match params.as_deref() {
                None => serde_json::json!({}),
                Some("-") => {
                    let mut s = String::new();
                    std::io::stdin().read_to_string(&mut s)?;
                    serde_json::from_str(&s)
                        .context("the parameters from stdin are not valid JSON")?
                }
                Some(p) => serde_json::from_str(p).context("the parameters are not valid JSON")?,
            };
            let resp = call(&socket_path, &method, params)?;
            println!("{}", json_text(serde_json::to_string_pretty(&resp)?));
            Ok(())
        }
    }
}

/// Pretty JSON for the terminal: the value as is, with DEL, C1 and bidi
/// controls as `\uXXXX` escapes (serde_json escapes only C0).
fn json_text(pretty: String) -> String {
    tiza_core::printable::escape_json_controls(&pretty).into_owned()
}

fn call(
    path: &std::path::Path,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let resp = tiza_tui::ipc::call(path, method, params).with_context(|| {
        format!(
            "no Tiza instance is listening on {} (open `tiza <file>` or pass --socket/--session)",
            path.display()
        )
    })?;
    if let Some(e) = resp.error {
        bail!("{} ({})", e.message, e.code);
    }
    Ok(resp.result.unwrap_or(serde_json::Value::Null))
}

/// `path` relative to `base` (both absolute): `doc.tiza`, `../demo/doc.tiza`
/// (a live `.tape` names the document from its own folder). Paths
/// with nothing in common come back as they are.
fn relative_to(path: &Path, base: &Path) -> PathBuf {
    let p: Vec<_> = path.components().collect();
    let b: Vec<_> = base.components().collect();
    let common = p.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if common == 0 {
        return path.to_path_buf();
    }
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for c in &p[common..] {
        out.push(c);
    }
    out
}

/// `render --all-steps --out DIR/`: one file per step, step 0
/// included, every canvas padded to the same size (the stable geometry of
/// a take), named `step-00.png` … so they sort in step order.
#[allow(clippy::too_many_arguments)]
fn render_all_steps(
    doc: &tiza_core::Document,
    theme: &Theme,
    out: &str,
    format: Format,
    opts: &LayoutOptions,
    svg_opts: &tiza_export::SvgOptions,
    png_opts: &tiza_export::PngOptions,
    skip_zero: bool,
) -> anyhow::Result<()> {
    if !matches!(
        format,
        Format::Png | Format::Svg | Format::Txt | Format::Ansi | Format::Md
    ) {
        bail!(
            "--all-steps writes still images (png, svg, txt, ansi, md); a .{0} already holds every step: `--out demo.{0}` without --all-steps",
            format_ext(format)
        );
    }
    let dir = std::path::Path::new(out);
    if dir.is_file() {
        bail!("--all-steps needs a directory, and {out} is a file");
    }
    let last = tiza_core::check::last_step(doc);
    // `--skip-zero` leaves out step 0 (the bare code), as in a .gif.
    let first: u32 = if skip_zero { 1 } else { 0 };
    if first > last {
        bail!(
            "--skip-zero leaves nothing to write: this document only has step 0 (no annotations or moves)"
        );
    }
    std::fs::create_dir_all(dir).with_context(|| format!("could not create {out}"))?;
    // The take's geometry, once for every step.
    let geometry = opts
        .stable
        .then(|| tiza_core::layout::stable_geometry(doc, opts));
    let mut canvases: Vec<tiza_core::canvas::Canvas> = (0..=last)
        .map(|step| {
            tiza_core::layout::layout_with(
                doc,
                theme,
                &LayoutOptions {
                    step,
                    ..opts.clone()
                },
                geometry.as_ref(),
            )
            .canvas
        })
        .collect();
    tiza_core::anim::same_size(&mut canvases, theme);
    let digits = last.to_string().len().max(2);
    let ext = format_ext(format);
    let names: Vec<String> = (first..=last)
        .map(|step| format!("step-{step:0digits$}.{ext}"))
        .collect();
    let (stale, odd) = stale_step_files(dir, &names, digits);
    let mut size = String::new();
    for (step, canvas) in canvases.iter().enumerate().skip(first as usize) {
        let bytes: Vec<u8> = match format {
            Format::Txt => (canvas.to_plain_text() + "\n").into_bytes(),
            Format::Ansi => canvas.to_ansi().into_bytes(),
            Format::Md => canvas.to_markdown(doc.title.as_deref()).into_bytes(),
            _ => {
                let svg = tiza_export::to_svg(canvas, theme, svg_opts);
                if format == Format::Svg {
                    svg.into_bytes()
                } else {
                    let png = tiza_export::to_png(&svg, png_opts)
                        .context("could not rasterize the PNG")?;
                    if size.is_empty() && png.len() > 24 {
                        let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
                        let h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
                        size = format!(", {w}×{h} px");
                    }
                    png
                }
            }
        };
        let path = dir.join(format!("step-{step:0digits$}.{ext}"));
        write_atomically(&path.to_string_lossy(), &bytes)?;
    }
    // Images of an earlier run that this one did not overwrite (a document
    // that had more steps): a `step-%02d` glob would pick them up. Only
    // regular files of this format go; a failed removal is a warning.
    let (same, other): (Vec<String>, Vec<String>) = stale
        .into_iter()
        .partition(|f| f.ends_with(&format!(".{ext}")));
    let mut removed: Vec<String> = Vec::new();
    let mut kept: Vec<String> = Vec::new();
    let mut why = String::new();
    for f in same {
        match std::fs::remove_file(dir.join(&f)) {
            Ok(()) => removed.push(f),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                if why.is_empty() {
                    why = e.to_string();
                }
                kept.push(f);
            }
        }
    }
    let n = names.len();
    let steps = if first == last {
        format!("step {last}")
    } else {
        format!("steps {first}-{last}")
    };
    if n > 1 && !size.is_empty() {
        size.push_str(" each");
    }
    // File and folder names as found on disk: shown, never raw.
    let shown_dir = tiza_core::printable::printable_line(&dir.display().to_string()).into_owned();
    eprintln!(
        "wrote {n} file{} to {} ({steps}{size})",
        if n == 1 { "" } else { "s" },
        shown_dir
    );
    let left = |n: usize| if n == 1 { "it is" } else { "they are" };
    if !removed.is_empty() {
        eprintln!(
            "tiza: removed {} from an earlier run: {}",
            files_word(removed.len()),
            name_list(&removed)
        );
    }
    if !kept.is_empty() {
        eprintln!(
            "tiza: could not remove {} from an earlier run ({why}), left as {}: {}",
            files_word(kept.len()),
            left(kept.len()),
            name_list(&kept)
        );
    }
    if !other.is_empty() {
        eprintln!(
            "tiza: {} also holds {} of another run, left as {}: {}",
            shown_dir,
            files_word(other.len()),
            left(other.len()),
            name_list(&other)
        );
    }
    if !odd.is_empty() {
        eprintln!(
            "tiza: {} also holds {} named like a step that {} not a regular file, left as {}: {}",
            shown_dir,
            if odd.len() == 1 {
                "1 entry".to_string()
            } else {
                format!("{} entries", odd.len())
            },
            if odd.len() == 1 { "is" } else { "are" },
            left(odd.len()),
            name_list(&odd)
        );
    }
    Ok(())
}

fn files_word(n: usize) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

/// At most five names, then «and N more» (a directory can hold thousands).
fn name_list(names: &[String]) -> String {
    const SHOWN: usize = 5;
    let list = if names.len() <= SHOWN + 1 {
        names.join(", ")
    } else {
        format!(
            "{} and {} more",
            names[..SHOWN].join(", "),
            names.len() - SHOWN
        )
    };
    // Names found on disk: shown, never raw.
    tiza_core::printable::printable_line(&list).into_owned()
}

/// Whether `f` is a name this `--all-steps` run's scheme writes: `step-` +
/// exactly `digits` digits (the last step's width, never fewer than 2) +
/// `.` + a still format, lowercase. `step-1.png`, `STEP-05.PNG`,
/// `step-05.png.bak` or `step-5a.png` are the user's; `step-0123.png` next
/// to a 7-step take is a name only a 1000+ step document writes, so it is
/// not this run's to judge and stays, unnamed.
fn is_step_name(f: &str, digits: usize) -> bool {
    let Some(rest) = f.strip_prefix("step-") else {
        return false;
    };
    let Some((num, ext)) = rest.split_once('.') else {
        return false;
    };
    num.len() == digits
        && num.bytes().all(|b| b.is_ascii_digit())
        && ["png", "svg", "txt", "ansi", "md"].contains(&ext)
}

/// The entries of `dir` named like a step (any still format) that a run
/// writing `names` does not write, sorted: the regular files, and the rest
/// (directories, symlinks, …), which are never removed and are described
/// (`step-08.png (a directory)`). Names that are not UTF-8 are skipped.
fn stale_step_files(
    dir: &std::path::Path,
    names: &[String],
    digits: usize,
) -> (Vec<String>, Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let mut files: Vec<String> = Vec::new();
    let mut odd: Vec<(String, &str)> = Vec::new();
    for e in entries.filter_map(|e| e.ok()) {
        let Some(f) = e.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !is_step_name(&f, digits) || names.contains(&f) {
            continue;
        }
        // `file_type` does not follow symlinks.
        match e.file_type() {
            Ok(t) if t.is_file() => files.push(f),
            Ok(t) if t.is_dir() => odd.push((f, "a directory")),
            Ok(t) if t.is_symlink() => odd.push((f, "a symlink")),
            _ => odd.push((f, "not a regular file")),
        }
    }
    files.sort();
    odd.sort();
    let odd = odd
        .into_iter()
        .map(|(f, what)| format!("{f} ({what})"))
        .collect();
    (files, odd)
}

fn format_ext(f: Format) -> &'static str {
    match f {
        Format::Txt => "txt",
        Format::Ansi => "ansi",
        Format::Md => "md",
        Format::Svg => "svg",
        Format::Png => "png",
        Format::Cast => "cast",
        Format::Tape => "tape",
        Format::Gif => "gif",
        Format::Mp4 => "mp4",
        Format::Webm => "webm",
    }
}

/// Writes next to the target and renames, so an interrupted render never
/// leaves a half-written file behind.
fn write_atomically(out: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let path = std::path::Path::new(out);
    let tmp = path.with_file_name(format!(
        ".{}.tiza-tmp",
        path.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "out".into())
    ));
    std::fs::write(&tmp, bytes).with_context(|| format!("could not write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("could not write {out}"))
}

/// Where `ffmpeg` is: `$TIZA_FFMPEG`, or the first one on PATH. Tiza never
/// bundles, downloads or vendors it (ADR 0009 spirit: the binary that shows
/// your code stays what it is; a video is the one thing it delegates).
fn find_ffmpeg() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("TIZA_FFMPEG")
        && !p.trim().is_empty()
    {
        return Some(std::path::PathBuf::from(p));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("ffmpeg"))
        .find(|p| p.is_file())
}

/// MP4/WebM: the frames go to a temporary directory as PNGs with a concat
/// list carrying each one's duration, and `ffmpeg` muxes them with a fixed,
/// printed command (30 fps, even dimensions, yuv420p for MP4 so every
/// player opens it). Returns the size of the file written.
fn encode_video(
    frames: &[tiza_core::anim::Frame],
    theme: &Theme,
    svg_opts: &tiza_export::SvgOptions,
    png_opts: &tiza_export::PngOptions,
    format: Format,
    out: &str,
) -> anyhow::Result<usize> {
    let Some(ffmpeg) = find_ffmpeg() else {
        eprintln!(
            "tiza: ffmpeg not found on PATH (or in $TIZA_FFMPEG), and Tiza never bundles it.\n\
             Install it — macOS: `brew install ffmpeg`; Debian/Ubuntu: `sudo apt install ffmpeg`; Fedora: `sudo dnf install ffmpeg`; Windows: `winget install ffmpeg` — and run the same command again.\n\
             `.gif` and `.cast` need nothing: `tiza render … --out {}.gif`",
            std::path::Path::new(out).with_extension("").display()
        );
        std::process::exit(2);
    };
    let dir = std::env::temp_dir().join(format!("tiza-video-{}", std::process::id()));
    std::fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    let _cleanup = TempDir(dir.clone());
    let mut list = String::from("ffconcat version 1.0\n");
    let mut total_ms: u64 = 0;
    for (i, f) in frames.iter().enumerate() {
        let svg = tiza_export::to_svg(&f.canvas, theme, svg_opts);
        let png = tiza_export::to_png(&svg, png_opts).context("could not rasterize a frame")?;
        let name = format!("f{i:05}.png");
        std::fs::write(dir.join(&name), png)?;
        // The GIF's own timing of this frame: its 10 ms
        // unit and 20 ms floor, so typed frames add up to the same length
        // in every format, and `--interval 0` still gives each step a
        // frame instead of `-t 0` and a file with no video stream.
        let delay_ms = tiza_export::gif::frame_delay_ms(f.delay_ms);
        list.push_str(&format!(
            "file '{name}'\nduration {:.3}\n",
            delay_ms as f64 / 1000.0
        ));
        total_ms += delay_ms;
    }
    // The last frame is listed once more so it holds for its duration at
    // all: without the repeat, ffmpeg 6 emits it as a single frame (its
    // `duration` only places the next entry, and there is none). What the
    // repeat itself lasts depends on the ffmpeg: 6.x gives it one frame,
    // 7+ (9.0.2 from Homebrew) holds it as long as the previous entry, so
    // the last step came out twice as long — a `duration` on the repeated
    // entry changes nothing there. `-t` with the sum of every frame's
    // duration cuts the output at exactly the GIF's length on every
    // version, so the videos are not left to how each ffmpeg treats the
    // tail of a concat list.
    if let Some(last) = frames.len().checked_sub(1) {
        list.push_str(&format!("file 'f{last:05}.png'\n"));
    }
    std::fs::write(dir.join("frames.txt"), &list)?;
    let total = format!("{:.3}", total_ms as f64 / 1000.0);
    let out_path = std::path::Path::new(out);
    let tmp = out_path.with_file_name(format!(
        ".{}.tiza-tmp.{}",
        out_path
            .file_stem()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "out".into()),
        format_ext(format)
    ));
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        dir.join("frames.txt").to_string_lossy().into_owned(),
        "-vf".into(),
        "scale=trunc(iw/2)*2:trunc(ih/2)*2,fps=30".into(),
        "-t".into(),
        total,
    ];
    match format {
        // yuv420p spelled out for the WebM too: ffmpeg 6 picked it
        // on its own for the RGBA frames, a newer one keeps them as
        // `gbrap`, which libvpx refuses, and no file is written.
        Format::Webm => args.extend([
            "-c:v".into(),
            "libvpx-vp9".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-b:v".into(),
            "0".into(),
            "-crf".into(),
            "30".into(),
        ]),
        _ => args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-crf".into(),
            "18".into(),
            "-movflags".into(),
            "+faststart".into(),
        ]),
    }
    args.push(tmp.to_string_lossy().into_owned());
    eprintln!(
        "ffmpeg {}",
        args.iter()
            .map(|a| if a.contains(' ') || a.contains('(') {
                format!("'{a}'")
            } else {
                a.clone()
            })
            .collect::<Vec<_>>()
            .join(" ")
    );
    let status = std::process::Command::new(&ffmpeg)
        .args(&args)
        .status()
        .with_context(|| format!("could not run {}", ffmpeg.display()))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!(
            "ffmpeg failed ({status}); the frames were fine, so check the codec above is in your ffmpeg build"
        );
    }
    std::fs::rename(&tmp, out_path).with_context(|| format!("could not write {out}"))?;
    Ok(std::fs::metadata(out_path)
        .map(|m| m.len() as usize)
        .unwrap_or(0))
}

struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `:export file.svg|png` inside the TUI: the image stack lives in this binary.
fn image_exporter(
    canvas: &tiza_core::canvas::Canvas,
    theme: &Theme,
    format: tiza_tui::app::ImageFormat,
    title: Option<&str>,
) -> Result<Vec<u8>, String> {
    let is_svg = format == tiza_tui::app::ImageFormat::Svg;
    let svg = tiza_export::to_svg(
        canvas,
        theme,
        &tiza_export::SvgOptions {
            embed_fonts: is_svg,
            title: title.map(str::to_string),
            ..Default::default()
        },
    );
    if is_svg {
        return Ok(svg.into_bytes());
    }
    tiza_export::to_png(&svg, &tiza_export::PngOptions::default())
        .map_err(|e| format!("could not rasterize the PNG: {e}"))
}

/// `--lang`: replaces the detected language; warns when no grammar exists.
/// The document `scene put` sends: a `.tiza` document, a `scene get`
/// envelope (`{document, step, …}`: `document` is unwrapped and the
/// envelope's `step` kept), or — only with `--as-code` — any other file
/// shown as code. Anything else is refused, so a stray JSON file never
/// lands on screen as 300 lines of JSON.
fn document_for_put(file: &str, as_code: bool) -> anyhow::Result<tiza_core::Document> {
    let (text, base_dir) = if file == "-" {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        (text, std::env::current_dir().ok())
    } else {
        (
            std::fs::read_to_string(file).with_context(|| format!("could not read {file}"))?,
            PathBuf::from(file).parent().map(PathBuf::from),
        )
    };
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{')
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
    {
        if let Some(inner) = value.get("document").filter(|d| d.is_object()) {
            let mut document = fs::parse_document(&inner.to_string(), base_dir.as_deref())
                .with_context(|| {
                    format!("{file}: the `document` of the scene envelope is not a valid document")
                })?;
            if let Some(step) = value.get("step").and_then(|s| s.as_u64()) {
                document.step = step as u32;
            }
            return Ok(document);
        }
        if value.get("schema").is_some() || value.get("code").is_some() {
            return fs::parse_document(trimmed, base_dir.as_deref())
                .with_context(|| format!("{file}: not a valid .tiza document"));
        }
    }
    if !as_code {
        bail!(
            "{file} is neither a .tiza document nor a `scene get` envelope; to show it as code, pass --as-code"
        );
    }
    let opened = if file == "-" {
        fs::from_stdin(&text, None).context("could not read the code from stdin")?
    } else {
        fs::open(file).with_context(|| format!("could not open {file}"))?
    };
    Ok(opened.document)
}

fn override_language(doc: &mut tiza_core::Document, lang: Option<&str>) {
    if let Some(l) = lang {
        // A path (`--lang "${ZED_FILE##*.}"` on a file without extension
        // expands to the whole path): detect from its name and the code.
        if l.contains('/') || l.contains('\\') {
            doc.source.language =
                tiza_core::highlight::detect_language(Some(l), &doc.code).language;
            return;
        }
        let lang = normalize_language(l);
        if !has_grammar(&lang) && lang != "txt" {
            eprintln!(
                "tiza: no grammar for `{}`; the code is shown as plain text",
                tiza_core::printable::printable_text(&lang)
            );
        }
        doc.source.language = lang;
    }
}

fn run_tui(cli: Cli, socket_path: PathBuf) -> anyhow::Result<()> {
    let opened = match cli.file.as_deref() {
        Some(f) if f != "-" => fs::open(f).with_context(|| format!("could not open {f}"))?,
        explicit => {
            if explicit.is_none() && std::io::stdin().is_terminal() {
                bail!(
                    "give a file (tiza src/main.rs:12-48) or pipe code through stdin (pbpaste | tiza --lang rs)"
                );
            }
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            fs::from_stdin(&text, None).map_err(|e| anyhow::anyhow!("{e}"))?
        }
    };
    let mut opened = opened;
    override_language(&mut opened.document, cli.lang.as_deref());
    // `--padding` is for this session: it is not written into the document.
    let padding = match &cli.padding {
        Some(pad) => {
            let nums: Vec<u16> = pad
                .split(',')
                .map(|t| t.trim().parse::<u16>())
                .collect::<Result<_, _>>()
                .context("--padding expects V or V,H")?;
            match nums.as_slice() {
                [v] => Some((*v, *v * 2)),
                [v, h] => Some((*v, *h)),
                _ => bail!("--padding expects V or V,H"),
            }
        }
        None => None,
    };
    let focus = match &cli.focus {
        Some(f) => Some(tiza_tui::app::parse_range(f).map_err(|e| anyhow::anyhow!("{e}"))?),
        None => None,
    };
    if !std::io::stdout().is_terminal() {
        bail!("the TUI needs a terminal on stdout; use `tiza render` for text output");
    }
    tiza_tui::run(
        opened,
        tiza_tui::RunOptions {
            step: cli.step,
            theme: cli.theme.and_then(ThemeArg::fixed),
            theme_auto: cli.theme == Some(ThemeArg::Auto),
            theme_file: cli.theme_file.clone(),
            config_overrides: config_theme(),
            present: cli.present,
            socket: if cli.no_socket {
                None
            } else {
                Some(socket_path)
            },
            focus,
            padding,
            image_exporter: Some(Box::new(image_exporter)),
            smoke: cli.smoke,
            mouse: cli.mouse,
            follow: cli.follow,
            no_save: cli.no_save,
        },
    )?;
    Ok(())
}
