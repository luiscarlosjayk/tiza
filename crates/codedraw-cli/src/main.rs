//! `codedraw`: for humans and agents. Without a subcommand it opens the TUI;
//! `render` draws without opening the TUI; `mcp` starts the MCP server;
//! `scene`/`api` talk to the live instance. JSON by default for machine
//! output, `-` for stdin/stdout, notices on stderr and exit 1 on error.

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand, ValueEnum};
use codedraw_core::fs;
use codedraw_core::layout::{LayoutOptions, layout};
use codedraw_core::socket;
use codedraw_core::{Theme, ThemeName};

#[derive(Parser)]
#[command(name = "codedraw", version, about = "Draw on top of code in the terminal to explain it on video.", long_about = None)]
#[command(
    after_help = "Examples:\n  codedraw src/main.rs:12-48        open a range of lines\n  pbpaste | codedraw --lang rs       code from the clipboard\n  codedraw demo.codedraw --present   open in presentation mode\n  codedraw render demo.codedraw --out - --step 2\n  codedraw render demo.codedraw --out demo.png --step 3   PNG (or .svg) of a step\n  codedraw render --out shot.png   PNG of what the live instance shows\n  codedraw mcp                       MCP server for agents\n  codedraw schema api                JSON Schema of the local API (schema document = the .codedraw format)\n  cat demo.codedraw | codedraw render - --out -   `-` is stdin everywhere"
)]
struct Cli {
    /// Source file (`main.rs`, `main.rs:12-48`), `.codedraw` document, or `-` for stdin (code or a document). Without it, reads stdin when piped.
    file: Option<String>,
    /// Language when it cannot be inferred (rs, ts, py, go…)
    #[arg(long)]
    lang: Option<String>,
    /// Initial presentation step
    #[arg(long)]
    step: Option<u32>,
    /// Theme
    #[arg(long, value_enum)]
    theme: Option<ThemeArg>,
    /// Start in presentation mode (no UI)
    #[arg(long)]
    present: bool,
    /// Focus a range of lines on open (A-B)
    #[arg(long)]
    focus: Option<String>,
    /// Blank cells around the drawing for this session: `V` or `V,H` (`:pad` saves it in the document)
    #[arg(long)]
    padding: Option<String>,
    /// Path of the local API socket (defaults to $CODEDRAW_SOCKET or the session)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    /// Session name: allows several instances (defaults to "default")
    #[arg(long, global = true)]
    session: Option<String>,
    /// Do not expose the local API
    #[arg(long)]
    no_socket: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ThemeArg {
    Dark,
    Light,
}

impl From<ThemeArg> for ThemeName {
    fn from(t: ThemeArg) -> Self {
        match t {
            ThemeArg::Dark => ThemeName::Dark,
            ThemeArg::Light => ThemeName::Light,
        }
    }
}

#[derive(Clone, Copy, ValueEnum, PartialEq)]
enum Format {
    Txt,
    Ansi,
    Svg,
    Png,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render a document without opening the TUI: text, ANSI, SVG or PNG.
    /// Without FILE it exports what the live instance is showing.
    Render {
        /// `.codedraw` document (or source file) to draw; omit it to use the live instance
        file: Option<String>,
        /// Output: a path or `-` for stdout. The extension picks the format (`.svg`, `.png`) unless --format is given.
        #[arg(long, default_value = "-")]
        out: String,
        /// Step to show (default: the document's current step, or all of them for a file)
        #[arg(long)]
        step: Option<u32>,
        /// Width in columns (default: 100, or the live terminal's width)
        #[arg(long)]
        width: Option<usize>,
        #[arg(long, value_enum)]
        format: Option<Format>,
        #[arg(long, value_enum)]
        theme: Option<ThemeArg>,
        /// No line numbers
        #[arg(long)]
        no_numbers: bool,
        /// Language of code read from stdin (`render -`)
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
        /// PNG: device scale (2 = HiDPI / video)
        #[arg(long, default_value_t = 2.0)]
        scale: f32,
        /// SVG: do not embed the fonts (smaller file; relies on the viewer's fonts)
        #[arg(long)]
        no_embed_fonts: bool,
        /// PNG: also use the system fonts as a fallback (emoji, CJK). Off by default so the output is reproducible.
        #[arg(long)]
        system_fonts: bool,
    },
    /// Start the MCP server (stdio) that controls the live instance
    Mcp,
    /// Read or change the scene of the live instance
    Scene {
        #[command(subcommand)]
        cmd: SceneCmd,
    },
    /// Call a raw local API method: `codedraw api status`, `codedraw api annotation.add '{...}'`
    Api {
        method: String,
        /// JSON parameters (or `-` to read them from stdin)
        params: Option<String>,
    },
    /// Print a JSON Schema: `document` (the .codedraw format) or `api` (every socket method)
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
        #[arg(long)]
        width: Option<u32>,
    },
    /// Instance status
    Status,
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("codedraw: {e:#}");
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
            width,
            format,
            theme,
            no_numbers,
            lang,
            font_size,
            margin,
            radius,
            scale,
            no_embed_fonts,
            system_fonts,
        }) => {
            let format = format.unwrap_or_else(|| {
                match std::path::Path::new(&out)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                {
                    Some("svg") => Format::Svg,
                    Some("png") => Format::Png,
                    Some("ansi") => Format::Ansi,
                    _ => Format::Txt,
                }
            });
            // The document: a file, or the live instance's scene.
            let (doc, step, width) = match &file {
                Some(f) => {
                    let opened = if f == "-" {
                        let mut text = String::new();
                        std::io::stdin().read_to_string(&mut text)?;
                        fs::from_stdin(&text, &lang_or_txt(lang.as_deref().or(cli.lang.as_deref())))
                            .context("could not read the document from stdin")?
                    } else {
                        fs::open(f).with_context(|| format!("could not open {f}"))?
                    };
                    if let Some(n) = &opened.notice {
                        eprintln!("codedraw: {n}");
                    }
                    (
                        opened.document,
                        step.unwrap_or(u32::MAX),
                        width.unwrap_or(100),
                    )
                }
                None => {
                    let mut scene: codedraw_core::protocol::Scene = serde_json::from_value(call(
                        &socket_path,
                        "scene.get",
                        serde_json::json!({}),
                    )?)
                    .context("unexpected scene.get response")?;
                    fs::resolve_code(&mut scene.document, None)
                        .context("the live document has no code")?;
                    let status: codedraw_core::protocol::Status = serde_json::from_value(call(
                        &socket_path,
                        "status",
                        serde_json::json!({}),
                    )?)
                    .context("unexpected status response")?;
                    let live_w = (status.cols as usize).saturating_sub(2).max(40);
                    (
                        scene.document,
                        step.unwrap_or(scene.step),
                        width.unwrap_or(live_w),
                    )
                }
            };
            let theme = Theme::from_name(
                theme
                    .map(Into::into)
                    .or(doc.theme)
                    .unwrap_or(ThemeName::Dark),
            );
            let l = layout(
                &doc,
                &theme,
                &LayoutOptions {
                    width,
                    step,
                    focus: None,
                    line_numbers: !no_numbers,
                    tab_width: 4,
                },
            );
            let bytes: Vec<u8> = match format {
                Format::Txt => (l.canvas.to_plain_text() + "\n").into_bytes(),
                Format::Ansi => l.canvas.to_ansi().into_bytes(),
                Format::Svg | Format::Png => {
                    let svg_opts = codedraw_export::SvgOptions {
                        font_size,
                        margin_cells: margin,
                        radius,
                        embed_fonts: format == Format::Svg && !no_embed_fonts,
                        title: doc.title.clone(),
                    };
                    let svg = codedraw_export::to_svg(&l.canvas, &theme, &svg_opts);
                    if format == Format::Svg {
                        svg.into_bytes()
                    } else {
                        codedraw_export::to_png(
                            &svg,
                            &codedraw_export::PngOptions {
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
                eprintln!("wrote {out} ({} bytes)", bytes.len());
            }
            Ok(())
        }
        Some(Cmd::Mcp) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(codedraw_mcp::serve(socket_path))
        }
        Some(Cmd::Scene { cmd }) => {
            let (method, params) = match cmd {
                SceneCmd::Get => ("scene.get", serde_json::json!({})),
                SceneCmd::List => ("scene.list", serde_json::json!({})),
                SceneCmd::Text { width } => ("render.text", serde_json::json!({"width": width})),
                SceneCmd::Status => ("status", serde_json::json!({})),
            };
            let resp = call(&socket_path, method, params)?;
            match method {
                "render.text" => println!(
                    "{}",
                    resp.get("text").and_then(|t| t.as_str()).unwrap_or("")
                ),
                "scene.list" => {
                    for l in resp.as_array().into_iter().flatten() {
                        println!("{}", l.as_str().unwrap_or(""));
                    }
                }
                _ => println!("{}", serde_json::to_string_pretty(&resp)?),
            }
            Ok(())
        }
        Some(Cmd::Schema { kind, out }) => {
            let value = match kind {
                SchemaKind::Document => codedraw_core::schema::document_schema(),
                SchemaKind::Api => serde_json::to_value(codedraw_core::schema::api_schema())?,
            };
            let text = serde_json::to_string_pretty(&value)? + "\n";
            if out == "-" {
                std::io::stdout().write_all(text.as_bytes())?;
            } else {
                std::fs::write(&out, text).with_context(|| format!("could not write {out}"))?;
                eprintln!("wrote {out}");
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
            println!("{}", serde_json::to_string_pretty(&resp)?);
            Ok(())
        }
    }
}

fn call(
    path: &std::path::Path,
    method: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let resp = codedraw_tui::ipc::call(path, method, params).with_context(|| {
        format!("no CodeDraw instance is listening on {} (open `codedraw <file>` or pass --socket/--session)", path.display())
    })?;
    if let Some(e) = resp.error {
        bail!("{} ({})", e.message, e.code);
    }
    Ok(resp.result.unwrap_or(serde_json::Value::Null))
}

fn lang_or_txt(lang: Option<&str>) -> String {
    lang.map(codedraw_core::highlight::normalize_language)
        .unwrap_or_else(|| "txt".into())
}

fn run_tui(cli: Cli, socket_path: PathBuf) -> anyhow::Result<()> {
    let opened = match cli.file.as_deref() {
        Some(f) if f != "-" => fs::open(f).with_context(|| format!("could not open {f}"))?,
        explicit => {
            if explicit.is_none() && std::io::stdin().is_terminal() {
                bail!(
                    "give a file (codedraw src/main.rs:12-48) or pipe code through stdin (pbpaste | codedraw --lang rs)"
                );
            }
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            fs::from_stdin(&text, &lang_or_txt(cli.lang.as_deref()))
                .map_err(|e| anyhow::anyhow!("{e}"))?
        }
    };
    let mut opened = opened;
    if let Some(l) = &cli.lang {
        opened.document.source.language = codedraw_core::highlight::normalize_language(l);
    }
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
        Some(f) => Some(codedraw_tui::app::parse_range(f).map_err(|e| anyhow::anyhow!("{e}"))?),
        None => None,
    };
    if !std::io::stdout().is_terminal() {
        bail!("the TUI needs a terminal on stdout; use `codedraw render` for text output");
    }
    codedraw_tui::run(
        opened,
        codedraw_tui::RunOptions {
            step: cli.step,
            theme: cli.theme.map(Into::into),
            present: cli.present,
            socket: if cli.no_socket {
                None
            } else {
                Some(socket_path)
            },
            focus,
            padding,
        },
    )?;
    Ok(())
}
