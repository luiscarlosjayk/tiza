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
    after_help = "Examples:\n  codedraw src/main.rs:12-48        open a range of lines\n  pbpaste | codedraw --lang rs       code from the clipboard\n  codedraw demo.codedraw --present   open in presentation mode\n  codedraw render demo.codedraw --out - --step 2\n  codedraw mcp                       MCP server for agents"
)]
struct Cli {
    /// Source file (`main.rs`, `main.rs:12-48`) or `.codedraw` document. Without it, reads stdin.
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

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Txt,
    Ansi,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render a document without opening the TUI (plain text or ANSI)
    Render {
        /// `.codedraw` document (or source file) to draw
        file: String,
        /// Output: a path or `-` for stdout
        #[arg(long, default_value = "-")]
        out: String,
        #[arg(long)]
        step: Option<u32>,
        #[arg(long, default_value_t = 100)]
        width: usize,
        #[arg(long, value_enum, default_value_t = Format::Txt)]
        format: Format,
        #[arg(long, value_enum)]
        theme: Option<ThemeArg>,
        /// No line numbers
        #[arg(long)]
        no_numbers: bool,
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
        }) => {
            let opened = fs::open(&file).with_context(|| format!("could not open {file}"))?;
            let doc = opened.document;
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
                    step: step.unwrap_or(u32::MAX),
                    focus: None,
                    line_numbers: !no_numbers,
                    tab_width: 4,
                },
            );
            let text = match format {
                Format::Txt => l.canvas.to_plain_text() + "\n",
                Format::Ansi => l.canvas.to_ansi(),
            };
            if out == "-" {
                std::io::stdout().write_all(text.as_bytes())?;
            } else {
                std::fs::write(&out, text).with_context(|| format!("could not write {out}"))?;
                eprintln!("wrote {out}");
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

fn run_tui(cli: Cli, socket_path: PathBuf) -> anyhow::Result<()> {
    let opened = match &cli.file {
        Some(f) => fs::open(f).with_context(|| format!("could not open {f}"))?,
        None => {
            if std::io::stdin().is_terminal() {
                bail!(
                    "give a file (codedraw src/main.rs:12-48) or pipe code through stdin (pbpaste | codedraw --lang rs)"
                );
            }
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            if text.trim().is_empty() {
                bail!("empty stdin");
            }
            let lang = cli
                .lang
                .as_deref()
                .map(codedraw_core::highlight::normalize_language)
                .unwrap_or_else(|| "txt".into());
            fs::from_stdin_text(&text, &lang)
        }
    };
    let mut opened = opened;
    if let Some(l) = &cli.lang {
        opened.document.source.language = codedraw_core::highlight::normalize_language(l);
    }
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
        },
    )?;
    Ok(())
}
