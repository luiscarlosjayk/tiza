//! `codedraw`: para humanos y agentes. Sin subcomando abre la TUI; `render`
//! dibuja sin abrir la TUI; `mcp` arranca el servidor MCP; `scene`/`api`
//! hablan con la instancia viva. JSON por defecto en la salida de máquina,
//! `-` para stdin/stdout, avisos a stderr y exit 1 en error.

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand, ValueEnum};
use codedraw_core::fs;
use codedraw_core::layout::{LayoutOptions, layout};
use codedraw_core::socket;
use codedraw_core::{Theme, ThemeName};

#[derive(Parser)]
#[command(name = "codedraw", version, about = "Dibuja encima de código en la terminal para explicarlo en video.", long_about = None)]
#[command(
    after_help = "Ejemplos:\n  codedraw src/main.rs:12-48        abre un rango de líneas\n  pbpaste | codedraw --lang rs       código desde el portapapeles\n  codedraw demo.codedraw --present   abre en modo presentación\n  codedraw render demo.codedraw --out - --step 2\n  codedraw mcp                       servidor MCP para agentes"
)]
struct Cli {
    /// Archivo fuente (`main.rs`, `main.rs:12-48`) o documento `.codedraw`. Sin él, lee stdin.
    file: Option<String>,
    /// Lenguaje cuando no se puede deducir (rs, ts, py, go…)
    #[arg(long)]
    lang: Option<String>,
    /// Paso inicial de la presentación
    #[arg(long)]
    step: Option<u32>,
    /// Tema
    #[arg(long, value_enum)]
    theme: Option<ThemeArg>,
    /// Arrancar en modo presentación (sin UI)
    #[arg(long)]
    present: bool,
    /// Enfocar un rango de líneas al abrir (A-B)
    #[arg(long)]
    focus: Option<String>,
    /// Ruta del socket de la API local (por defecto $CODEDRAW_SOCKET o la sesión)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    /// Nombre de sesión: permite varias instancias (por defecto "default")
    #[arg(long, global = true)]
    session: Option<String>,
    /// No exponer la API local
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
    /// Renderiza un documento sin abrir la TUI (texto plano o ANSI)
    Render {
        /// Documento `.codedraw` (o fuente) a dibujar
        file: String,
        /// Salida: ruta o `-` para stdout
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
        /// Sin números de línea
        #[arg(long)]
        no_numbers: bool,
    },
    /// Arranca el servidor MCP (stdio) que controla la instancia viva
    Mcp,
    /// Lee o cambia la escena de la instancia viva
    Scene {
        #[command(subcommand)]
        cmd: SceneCmd,
    },
    /// Llama a un método crudo de la API local: `codedraw api status`, `codedraw api annotation.add '{...}'`
    Api {
        method: String,
        /// Parámetros JSON (o `-` para leerlos de stdin)
        params: Option<String>,
    },
}

#[derive(Subcommand)]
enum SceneCmd {
    /// Imprime el documento completo (JSON)
    Get,
    /// Lista las anotaciones
    List,
    /// Muestra lo que se ve en pantalla como texto
    Text {
        #[arg(long)]
        width: Option<u32>,
    },
    /// Estado de la instancia
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
            let opened = fs::open(&file).with_context(|| format!("no se pudo abrir {file}"))?;
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
                std::fs::write(&out, text).with_context(|| format!("no se pudo escribir {out}"))?;
                eprintln!("escrito {out}");
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
                        .context("los parámetros de stdin no son JSON válido")?
                }
                Some(p) => serde_json::from_str(p).context("los parámetros no son JSON válido")?,
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
        format!("no hay una instancia de CodeDraw escuchando en {} (abre `codedraw <archivo>` o pasa --socket/--session)", path.display())
    })?;
    if let Some(e) = resp.error {
        bail!("{} ({})", e.message, e.code);
    }
    Ok(resp.result.unwrap_or(serde_json::Value::Null))
}

fn run_tui(cli: Cli, socket_path: PathBuf) -> anyhow::Result<()> {
    let opened = match &cli.file {
        Some(f) => fs::open(f).with_context(|| format!("no se pudo abrir {f}"))?,
        None => {
            if std::io::stdin().is_terminal() {
                bail!(
                    "indica un archivo (codedraw src/main.rs:12-48) o pasa código por stdin (pbpaste | codedraw --lang rs)"
                );
            }
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            if text.trim().is_empty() {
                bail!("stdin vacío");
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
        bail!("la TUI necesita una terminal en stdout; usa `codedraw render` para salida de texto");
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
