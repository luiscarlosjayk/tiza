//! `install.sh` at the root of the repository, the one-line install:
//! `curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh`.
//!
//! Acceptance criteria:
//! - AC1 POSIX sh: it runs with `sh` and with `dash`, from a file and from
//!   a pipe as `curl | sh` runs it, and `shellcheck -s sh` finds nothing.
//! - AC2 `uname -s` and `uname -m` (arm64/aarch64, x86_64/amd64) give the
//!   three targets of release.yml. On Apple silicon (`hw.optional.arm64`),
//!   an x86_64 uname (Rosetta 2) gets the arm64 binary.
//! - AC3 On Linux it checks glibc 2.35 (`getconf`, else `ldd`). An older
//!   glibc, musl or a platform without a binary (Linux aarch64, BSD) stops
//!   with `cargo install tiza`, and Windows (MSYS, MinGW, Cygwin) with
//!   WSL2: exit code 1, before any download.
//! - AC4 The version is the target of the releases/latest redirect (no
//!   API, no prereleases), also with `location` in ~/.curlrc;
//!   `TIZA_VERSION=v0.1.0` (or `0.1.0`) pins one and does not ask for the
//!   latest.
//! - AC5 It downloads `tiza-TAG-TARGET.tar.gz` and its `.sha256`, the
//!   names of release.yml, and checks them with sha256sum or shasum: a
//!   checksum that does not match installs nothing and exits 1.
//! - AC6 It installs into `${TIZA_INSTALL_DIR:-$HOME/.local/bin}` with
//!   mode 755, after it checks the folder and before any download. It
//!   prints the exact line that adds the folder to PATH when it is not in
//!   it, also for a folder name with a space, `$`, `"` or a backquote.
//! - AC7 Its work folder (`mktemp -d`) goes away on exit, also after an
//!   error or a signal. curl runs with `--proto` and `--tlsv1.2`; wget, when
//!   there is no curl, with TLS 1.2, three tries and no HSTS file; no sudo.
//! - AC8 It runs the new binary (`--version`) before it replaces the old
//!   one: one that does not start leaves the old one. It ends with
//!   `tiza --version` of the installed binary and a next step; `--help`
//!   and `-h` list the variables. A macOS quarantine flag is removed. No
//!   command reads the rest of a script that comes through a pipe.
//! - AC9 The README gives the command of `--help` as the first way to
//!   install, with what it does and its two variables; the archives and
//!   `cargo install tiza` come after it.
//!
//! The releases come from a local HTTP server (`TIZA_DOWNLOAD_BASE`, for
//! tests only), and the platform from fake `uname`, `getconf`, `ldd`,
//! `sysctl` and `xattr` first in PATH: no real network and no sleeps.

use std::collections::HashMap;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use tiza_qa::scratch;

/// The targets of release.yml, each with the `uname -s` and `uname -m`
/// that pick it.
const TARGETS: [(&str, &str, &str); 3] = [
    ("Darwin", "arm64", "aarch64-apple-darwin"),
    ("Darwin", "x86_64", "x86_64-apple-darwin"),
    ("Linux", "x86_64", "x86_64-unknown-linux-gnu"),
];

const LINUX: &str = "x86_64-unknown-linux-gnu";
const CARGO: &str = "Install Tiza from source with Rust (https://rustup.rs): cargo install tiza";
const HTTP_WARNING: &str = "warning: TIZA_DOWNLOAD_BASE uses http. Use it only for tests.\n";

/// How long one run of the script may take before the test kills it.
const DEADLINE: Duration = Duration::from_secs(60);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap()
}

fn has(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd}"))
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The names in `dir`, sorted; none when it does not exist.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

// --- The server -----------------------------------------------------------

enum Reply {
    Body(Vec<u8>),
    Redirect(String),
    /// No answer: tells the first channel that the request came, then
    /// closes the connection when the second one gets a message.
    Hold(mpsc::Sender<()>, mpsc::Receiver<()>),
}

/// GitHub Releases on 127.0.0.1: `base` is `http://127.0.0.1:PORT/releases`.
struct Server {
    base: String,
    hits: Arc<Mutex<Vec<String>>>,
}

impl Server {
    /// Answers the routes that `routes(base)` gives (paths under
    /// `/releases/`), one connection after the other, until the test ends.
    /// Any other path gets a 404.
    fn start(routes: impl FnOnce(&str) -> Vec<(String, Reply)>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let base = format!("http://127.0.0.1:{port}/releases");
        let routes: HashMap<String, Reply> = routes(&base)
            .into_iter()
            .map(|(path, reply)| (format!("/releases/{path}"), reply))
            .collect();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = answer(stream, &routes, &log);
            }
        });
        Server { base, hits }
    }

    /// The paths asked for since the last call.
    fn take_hits(&self) -> Vec<String> {
        std::mem::take(&mut *self.hits.lock().unwrap())
    }
}

fn answer(
    mut stream: TcpStream,
    routes: &HashMap<String, Reply>,
    hits: &Mutex<Vec<String>>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim_end().is_empty() {
            break;
        }
    }
    let path = request
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    hits.lock().unwrap().push(path.clone());
    let (status, location, body): (&str, &str, &[u8]) = match routes.get(&path) {
        Some(Reply::Body(body)) => ("200 OK", "", body),
        Some(Reply::Redirect(to)) => ("302 Found", to, b""),
        Some(Reply::Hold(arrived, release)) => {
            let _ = arrived.send(());
            let _ = release.recv_timeout(DEADLINE);
            return Ok(());
        }
        None => ("404 Not Found", "", b"not found\n"),
    };
    let location = if location.is_empty() {
        String::new()
    } else {
        format!("Location: {location}\r\n")
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\n{location}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

// --- Fake releases --------------------------------------------------------

/// The archive of `tag` for `target`, packed as release.yml packs it:
/// `tiza-TAG-TARGET/tiza` (here a script that reads all of stdin, then
/// prints `tiza VERSION (fake TARGET)`) and a README.
fn archive(work: &Path, tag: &str, target: &str) -> Vec<u8> {
    let version = tag.trim_start_matches('v');
    let tiza = format!("#!/bin/sh\ncat >/dev/null\necho 'tiza {version} (fake {target})'\n");
    pack(work, tag, target, &tiza)
}

/// An archive of `tag` for `target` whose `tiza` is the script `tiza`.
fn pack(work: &Path, tag: &str, target: &str, tiza: &str) -> Vec<u8> {
    let name = format!("tiza-{tag}-{target}");
    let stage = work.join("stage").join(&name);
    std::fs::create_dir_all(stage.join(&name)).unwrap();
    let bin = stage.join(&name).join("tiza");
    std::fs::write(&bin, tiza).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(stage.join(&name).join("README.md"), "fake release\n").unwrap();
    let out = stage.join("out.tar.gz");
    let ok = Command::new("tar")
        .env("COPYFILE_DISABLE", "1")
        .arg("-czf")
        .arg(&out)
        .arg("-C")
        .arg(&stage)
        .arg(&name)
        .status()
        .unwrap()
        .success();
    assert!(ok, "tar of {name}");
    std::fs::read(&out).unwrap()
}

/// The SHA-256 of `bytes` in hex, from sha256sum or shasum.
fn sha256(work: &Path, bytes: &[u8]) -> String {
    let file = work.join("hash-me");
    std::fs::write(&file, bytes).unwrap();
    let out = Command::new("sha256sum")
        .arg(&file)
        .output()
        .or_else(|_| {
            Command::new("shasum")
                .args(["-a", "256"])
                .arg(&file)
                .output()
        })
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    text.split_whitespace().next().unwrap().to_string()
}

/// The routes of release `tag` for every target: each archive and its
/// `.sha256`, `HASH  NAME` as `shasum -a 256` writes it in release.yml.
fn release(work: &Path, tag: &str) -> Vec<(String, Reply)> {
    let mut routes = Vec::new();
    for (_, _, target) in TARGETS {
        let name = format!("tiza-{tag}-{target}.tar.gz");
        let bytes = archive(work, tag, target);
        let line = format!("{}  {name}\n", sha256(work, &bytes));
        routes.push((format!("download/{tag}/{name}"), Reply::Body(bytes)));
        routes.push((
            format!("download/{tag}/{name}.sha256"),
            Reply::Body(line.into_bytes()),
        ));
    }
    routes
}

/// releases/latest as GitHub answers it: a redirect to releases/tag/TAG,
/// the page of the release.
fn latest(base: &str, tag: &str) -> [(String, Reply); 2] {
    [
        (
            "latest".into(),
            Reply::Redirect(format!("{base}/tag/{tag}")),
        ),
        (
            format!("tag/{tag}"),
            Reply::Body(b"<html>release</html>".to_vec()),
        ),
    ]
}

/// The paths of the latest release with curl, which follows the redirect.
fn found_latest(tag: &str) -> Vec<String> {
    vec!["/releases/latest".into(), format!("/releases/tag/{tag}")]
}

/// Release v0.2.0, which releases/latest names.
fn v020(work: &Path, base: &str) -> Vec<(String, Reply)> {
    let mut routes = release(work, "v0.2.0");
    routes.extend(latest(base, "v0.2.0"));
    routes
}

/// The paths of one install of `tag` for `target`.
fn downloads(tag: &str, target: &str) -> Vec<String> {
    let file = format!("/releases/download/{tag}/tiza-{tag}-{target}.tar.gz");
    vec![file.clone(), format!("{file}.sha256")]
}

// --- The fake platform ----------------------------------------------------

/// What the fake tools first in PATH say about the machine.
#[derive(Clone, Copy)]
struct Platform {
    os: &'static str,
    arch: &'static str,
    /// What `getconf GNU_LIBC_VERSION` prints; `None`: it fails, as with musl.
    getconf: Option<&'static str>,
    /// The first line of `ldd --version`; `None`: ldd prints nothing.
    ldd: Option<&'static str>,
    /// `sysctl -n hw.optional.arm64` is 1, as on Apple silicon (also under
    /// Rosetta 2); else sysctl does not know the key, as on an Intel Mac.
    apple_silicon: bool,
    /// `xattr -p com.apple.quarantine` finds the flag.
    quarantine: bool,
}

impl Platform {
    fn new(os: &'static str, arch: &'static str) -> Platform {
        Platform {
            os,
            arch,
            getconf: Some("glibc 2.35"),
            ldd: None,
            apple_silicon: false,
            quarantine: false,
        }
    }

    fn of(target: &str) -> Platform {
        let (os, arch, _) = TARGETS.into_iter().find(|t| t.2 == target).unwrap();
        Platform::new(os, arch)
    }

    fn linux() -> Platform {
        Platform::of(LINUX)
    }
}

/// Writes the fake tools of `p` into `dir/fake-bin`. `xattr` adds each of
/// its calls to `dir/xattr.log`.
fn fake_tools(dir: &Path, p: Platform) -> PathBuf {
    let bin = dir.join("fake-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let getconf = match p.getconf {
        Some(v) => format!("[ \"$1\" = GNU_LIBC_VERSION ] || exit 1\necho '{v}'"),
        None => "exit 1".into(),
    };
    // musl's ldd writes to stderr and exits 1; the script reads both streams.
    let ldd = match p.ldd {
        Some(line) => format!("echo '{line}' >&2\nexit 1"),
        None => "exit 1".into(),
    };
    let xattr = format!(
        "echo \"$*\" >> '{}'\n[ \"$1\" = -p ] && exit {}\nexit 0",
        s(&dir.join("xattr.log")),
        if p.quarantine { 0 } else { 1 }
    );
    let tools = [
        (
            "uname",
            format!(
                "case $1 in -m) echo '{}' ;; *) echo '{}' ;; esac",
                p.arch, p.os
            ),
        ),
        ("getconf", getconf),
        ("ldd", ldd),
        (
            "sysctl",
            format!(
                "[ \"$*\" = '-n hw.optional.arm64' ] && {}\necho \"sysctl: unknown oid '$2'\" >&2\nexit 1",
                if p.apple_silicon {
                    "echo 1 && exit 0"
                } else {
                    "false"
                }
            ),
        ),
        ("xattr", xattr),
    ];
    for (name, body) in tools {
        let path = bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    bin
}

/// A folder of links to every command in PATH except `without`: the PATH
/// of this machine without those commands.
fn path_without(dir: &Path, name: &str, without: &[&str]) -> PathBuf {
    let farm = dir.join(name);
    std::fs::create_dir_all(&farm).unwrap();
    for folder in std::env::split_paths(&std::env::var_os("PATH").unwrap()) {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = entry.file_name();
            let link = farm.join(&file);
            // The first folder of PATH wins, as in a shell.
            if without.iter().any(|w| file == *w) || link.symlink_metadata().is_ok() {
                continue;
            }
            let _ = std::os::unix::fs::symlink(entry.path(), &link);
        }
    }
    farm
}

// --- One test machine -----------------------------------------------------

/// A scratch folder with HOME, TMPDIR and the fake tools, and a server.
struct Machine {
    dir: PathBuf,
    home: PathBuf,
    tmp: PathBuf,
    fake_bin: PathBuf,
    server: Server,
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl fmt::Display for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "exit code {:?}\n--- stdout\n{}--- stderr\n{}",
            self.code, self.stdout, self.stderr
        )
    }
}

impl Run {
    fn ok(&self) -> &Run {
        assert_eq!(self.code, Some(0), "{self}");
        self
    }

    fn fails(&self, code: i32, lines: &[&str]) -> &Run {
        assert_eq!(self.code, Some(code), "{self}");
        for line in lines {
            assert!(self.stderr.contains(line), "no {line:?} on stderr\n{self}");
        }
        self
    }
}

impl Machine {
    fn new(
        name: &str,
        platform: Platform,
        routes: impl FnOnce(&Path, &str) -> Vec<(String, Reply)>,
    ) -> Machine {
        let dir = std::fs::canonicalize(scratch(name)).unwrap();
        let home = dir.join("home");
        let tmp = dir.join("tmp");
        let work = dir.join("work");
        for d in [&home, &tmp, &work] {
            std::fs::create_dir_all(d).unwrap();
        }
        let fake_bin = fake_tools(&dir, platform);
        let server = Server::start(|base| routes(&work, base));
        Machine {
            dir,
            home,
            tmp,
            fake_bin,
            server,
        }
    }

    /// PATH: the fake tools, then `front`, then the PATH of this machine.
    fn path(&self, front: &[&Path]) -> String {
        let mut dirs = vec![self.fake_bin.clone()];
        dirs.extend(front.iter().map(|d| d.to_path_buf()));
        dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        std::env::join_paths(dirs).unwrap().into_string().unwrap()
    }

    /// Starts `shell` on the script in `self.dir`, with a clean environment
    /// and `env` on top. With `pipe`, the shell reads the script from stdin
    /// as `curl … | sh` does (`args` after `-s --`), then `after`; else it
    /// gets the file.
    fn spawn(
        &self,
        shell: &str,
        pipe: bool,
        args: &[&str],
        env: &[(&str, &str)],
        after: &[u8],
    ) -> Child {
        let script = root().join("install.sh");
        let mut cmd = Command::new(shell);
        if pipe {
            if !args.is_empty() {
                cmd.args(["-s", "--"]);
            }
        } else {
            cmd.arg(&script);
        }
        cmd.args(args)
            .current_dir(&self.dir)
            .env_clear()
            .env("PATH", self.path(&[]))
            .env("HOME", &self.home)
            .env("TMPDIR", &self.tmp)
            .env("SHELL", "/bin/sh")
            .env("TIZA_DOWNLOAD_BASE", &self.server.base)
            .envs(env.iter().copied())
            .stdin(if pipe { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        if let Some(mut stdin) = child.stdin.take() {
            let mut text = std::fs::read(&script).unwrap();
            text.extend_from_slice(after);
            std::thread::spawn(move || {
                let _ = stdin.write_all(&text);
            });
        }
        child
    }

    fn run(&self, shell: &str, pipe: bool, args: &[&str], env: &[(&str, &str)]) -> Run {
        finish(self.spawn(shell, pipe, args, env, b""))
    }

    /// `curl … | sh` with `env`.
    fn pipe(&self, env: &[(&str, &str)]) -> Run {
        self.run("sh", true, &[], env)
    }
}

/// Waits for `child` with its output drained on two threads; kills it and
/// fails the test after [`DEADLINE`].
fn finish(mut child: Child) -> Run {
    let start = Instant::now();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let drain = |mut r: Box<dyn Read + Send>, tx: mpsc::Sender<()>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            let _ = tx.send(());
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let out = drain(Box::new(child.stdout.take().unwrap()), done_tx.clone());
    let err = drain(Box::new(child.stderr.take().unwrap()), done_tx);
    for _ in 0..2 {
        if done_rx
            .recv_timeout(DEADLINE.saturating_sub(start.elapsed()))
            .is_err()
        {
            let _ = child.kill();
            panic!("install.sh did not end in {DEADLINE:?}");
        }
    }
    let status = child.wait().unwrap();
    Run {
        code: status.code(),
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
    }
}

// --- AC1 ------------------------------------------------------------------

#[test]
fn runs_with_sh_and_dash_from_a_pipe_and_from_a_file() {
    let mut shells = vec!["sh"];
    if has("dash") {
        shells.push("dash");
    } else {
        eprintln!("dash is not in PATH: only sh runs the script");
    }
    let m = Machine::new("shells", Platform::linux(), v020);
    for shell in shells {
        for pipe in [true, false] {
            let bin = m.dir.join(format!("bin-{shell}-{pipe}"));
            let run = m.run(shell, pipe, &[], &[("TIZA_INSTALL_DIR", s(&bin))]);
            run.ok();
            assert!(
                run.stdout
                    .contains("\ntiza 0.2.0 (fake x86_64-unknown-linux-gnu)\n"),
                "{shell}, pipe {pipe}\n{run}"
            );
            assert_eq!(names(&bin), ["tiza"], "{shell}, pipe {pipe}");
        }
    }
}

#[test]
fn nothing_reads_the_rest_of_a_piped_script() {
    // bash reads a script from a pipe one byte at a time, so a command that
    // reads stdin (the fake tiza does) takes the line after the script.
    if !has("bash") {
        eprintln!("bash is not in PATH: skipped");
        return;
    }
    let m = Machine::new("stdin", Platform::linux(), v020);
    let bin = m.dir.join("bin");
    let child = m.spawn(
        "bash",
        true,
        &[],
        &[("TIZA_INSTALL_DIR", s(&bin))],
        b"echo LEFTOVER\n",
    );
    let run = finish(child);
    run.ok();
    assert!(run.stdout.ends_with("\nLEFTOVER\n"), "{run}");
}

#[test]
fn shellcheck_finds_nothing() {
    if !has("shellcheck") {
        eprintln!("shellcheck is not in PATH: skipped");
        return;
    }
    let out = Command::new("shellcheck")
        .args(["-s", "sh"])
        .arg(root().join("install.sh"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

// --- AC2, AC6, AC8: the whole install -------------------------------------

#[test]
fn installs_the_latest_release_for_each_target_into_home_local_bin() {
    for (os, arch, target) in TARGETS {
        let m = Machine::new(&format!("latest-{target}"), Platform::new(os, arch), v020);
        let run = m.pipe(&[]);
        run.ok();
        let bin = m.home.join(".local/bin");
        let base = &m.server.base;
        let b = s(&bin);
        let home = s(&m.home);
        assert_eq!(
            run.stdout,
            format!(
                "Platform: {target}\n\
                 Version: v0.2.0 (the latest release)\n\
                 Download: {base}/download/v0.2.0/tiza-v0.2.0-{target}.tar.gz\n\
                 Checksum: OK (SHA-256)\n\
                 Installed: {b}/tiza\n\
                 {b} is not in PATH. Add this line to {home}/.profile, then open a new terminal:\n  \
                 export PATH=\"{b}:$PATH\"\n\
                 tiza 0.2.0 (fake {target})\n\
                 Next: run \"tiza FILE\" to open a file, or \"tiza --help\" for the options.\n"
            ),
            "{target}\n{run}"
        );
        assert_eq!(run.stderr, HTTP_WARNING, "{target}");
        assert_eq!(names(&bin), ["tiza"], "{target}: only the binary");
        let mode = std::fs::metadata(bin.join("tiza"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "{target}");
        assert_eq!(
            names(&m.tmp),
            Vec::<String>::new(),
            "{target}: work folder left"
        );
        let mut hits = found_latest("v0.2.0");
        hits.extend(downloads("v0.2.0", target));
        assert_eq!(m.server.take_hits(), hits, "{target}");
    }
}

#[test]
fn apple_silicon_gets_the_arm64_binary_and_loses_a_quarantine_flag() {
    // Under Rosetta 2, uname -m says x86_64.
    let rosetta = Platform {
        apple_silicon: true,
        quarantine: true,
        ..Platform::new("Darwin", "x86_64")
    };
    let m = Machine::new("rosetta", rosetta, v020);
    let bin = m.dir.join("bin");
    let run = m.pipe(&[("TIZA_INSTALL_DIR", s(&bin))]);
    run.ok();
    assert!(
        run.stdout.starts_with("Platform: aarch64-apple-darwin\n"),
        "{run}"
    );
    assert!(
        run.stdout
            .contains("\ntiza 0.2.0 (fake aarch64-apple-darwin)\n"),
        "{run}"
    );
    // The flag goes before the first run of the new binary, which macOS
    // would stop, and before it replaces the old one.
    let log = std::fs::read_to_string(m.dir.join("xattr.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    let staged = format!("-p com.apple.quarantine {}/.tiza.install.", s(&bin));
    assert_eq!(lines.len(), 2, "{log}");
    assert!(lines[0].starts_with(&staged), "{log}");
    assert_eq!(lines[1], lines[0].replacen("-p", "-d", 1), "{log}");
    assert_eq!(names(&bin), ["tiza"]);

    // Without the flag, only the check; on Linux, not even that.
    let mac = Machine::new("no-quarantine", Platform::of("aarch64-apple-darwin"), v020);
    mac.pipe(&[("TIZA_INSTALL_DIR", s(&bin))]).ok();
    let log = std::fs::read_to_string(mac.dir.join("xattr.log")).unwrap();
    assert!(
        log.starts_with(&staged) && log.lines().count() == 1,
        "{log}"
    );
    let linux = Machine::new("linux-no-xattr", Platform::linux(), v020);
    linux.pipe(&[("TIZA_INSTALL_DIR", s(&bin))]).ok();
    assert!(!linux.dir.join("xattr.log").exists());
}

// --- AC3 ------------------------------------------------------------------

#[test]
fn platforms_without_a_binary_stop_before_any_download() {
    let linux = Platform::linux();
    let windows = [
        "Tiza does not support Windows.",
        "Install WSL2. Then run this command in a WSL2 terminal:",
        "  curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh",
    ];
    let cases: Vec<(Platform, Vec<&str>)> = vec![
        (
            Platform::new("Linux", "aarch64"),
            vec!["error: Tiza has no binary for Linux on aarch64.", CARGO],
        ),
        (
            Platform::new("Linux", "armv7l"),
            vec!["error: Tiza has no binary for Linux on armv7l.", CARGO],
        ),
        (
            Platform::new("FreeBSD", "amd64"),
            vec!["error: Tiza has no binary for FreeBSD on x86_64.", CARGO],
        ),
        (
            Platform::new("OpenBSD", "arm64"),
            vec!["error: Tiza has no binary for OpenBSD on aarch64.", CARGO],
        ),
        (
            Platform::new("MINGW64_NT-10.0-26100", "x86_64"),
            windows.to_vec(),
        ),
        (
            Platform::new("MSYS_NT-10.0-26100", "x86_64"),
            windows.to_vec(),
        ),
        (
            Platform::new("CYGWIN_NT-10.0-26100", "x86_64"),
            windows.to_vec(),
        ),
        (
            Platform {
                getconf: Some("glibc 2.31"),
                ..linux
            },
            vec![
                "error: This Linux has glibc 2.31. The Tiza binary needs glibc 2.35 or later.",
                CARGO,
            ],
        ),
        (
            Platform {
                getconf: Some("glibc 2.34"),
                ..linux
            },
            vec!["This Linux has glibc 2.34.", CARGO],
        ),
        (
            Platform {
                getconf: Some("glibc 1.99"),
                ..linux
            },
            vec!["This Linux has glibc 1.99.", CARGO],
        ),
        (
            Platform {
                getconf: None,
                ldd: Some("ldd (Ubuntu GLIBC 2.31-0ubuntu9.16) 2.31"),
                ..linux
            },
            vec!["This Linux has glibc 2.31.", CARGO],
        ),
        (
            Platform {
                getconf: None,
                ldd: Some("musl libc (x86_64)"),
                ..linux
            },
            vec![
                "error: This Linux uses musl libc. The Tiza binary needs glibc 2.35 or later.",
                CARGO,
            ],
        ),
    ];
    for (i, (platform, lines)) in cases.into_iter().enumerate() {
        let m = Machine::new(&format!("no-binary-{i}"), platform, v020);
        let run = m.pipe(&[]);
        run.fails(1, &lines);
        let label = format!("{} {}", platform.os, platform.arch);
        assert_eq!(run.stdout, "", "{label}");
        if platform.os.contains("_NT") {
            assert!(
                !run.stderr.contains("cargo install"),
                "{label}: no Windows build\n{run}"
            );
        }
        assert_eq!(
            m.server.take_hits(),
            Vec::<String>::new(),
            "{label}: a download"
        );
        assert!(!m.home.join(".local").exists(), "{label}");
        assert_eq!(names(&m.tmp), Vec::<String>::new(), "{label}");
    }
}

#[test]
fn glibc_2_35_or_later_installs_and_an_unknown_version_only_warns() {
    let linux = Platform::linux();
    let known = [
        Platform {
            getconf: Some("glibc 2.39"),
            ..linux
        },
        Platform {
            getconf: Some("glibc 3.0"),
            ..linux
        },
        Platform {
            getconf: None,
            ldd: Some("ldd (GNU libc) 2.36"),
            ..linux
        },
    ];
    for (i, platform) in known.into_iter().enumerate() {
        let m = Machine::new(&format!("glibc-{i}"), platform, v020);
        let run = m.pipe(&[]);
        run.ok();
        assert_eq!(run.stderr, HTTP_WARNING, "{run}");
    }
    let unknown = [
        Platform {
            getconf: None,
            ..linux
        },
        Platform {
            getconf: None,
            ldd: Some("ldd: unrecognized option '--version'"),
            ..linux
        },
        Platform {
            getconf: Some("glibc 2"),
            ..linux
        },
    ];
    for (i, platform) in unknown.into_iter().enumerate() {
        let m = Machine::new(&format!("glibc-unknown-{i}"), platform, v020);
        let run = m.pipe(&[]);
        run.ok();
        assert!(
            run.stderr.contains(
                "warning: Cannot find the glibc version. The Tiza binary needs glibc 2.35 or later.\n"
            ),
            "{run}"
        );
        assert!(
            run.stdout
                .contains("\ntiza 0.2.0 (fake x86_64-unknown-linux-gnu)\n")
        );
    }
}

// --- AC4 ------------------------------------------------------------------

#[test]
fn tiza_version_pins_a_release_and_does_not_ask_for_the_latest() {
    let m = Machine::new("pinned", Platform::linux(), |work, base| {
        let mut routes = v020(work, base);
        routes.extend(release(work, "v0.1.0"));
        routes
    });
    for version in ["v0.1.0", "0.1.0"] {
        let bin = m.dir.join(format!("bin-{version}"));
        let run = m.pipe(&[("TIZA_VERSION", version), ("TIZA_INSTALL_DIR", s(&bin))]);
        run.ok();
        assert!(
            run.stdout
                .contains("\nVersion: v0.1.0 (from TIZA_VERSION)\n"),
            "{run}"
        );
        assert!(
            run.stdout
                .contains("\ntiza 0.1.0 (fake x86_64-unknown-linux-gnu)\n"),
            "{run}"
        );
        assert_eq!(
            m.server.take_hits(),
            downloads("v0.1.0", LINUX),
            "{version}"
        );
    }

    let bin = m.dir.join("bin-missing");
    m.pipe(&[("TIZA_VERSION", "v9.9.9"), ("TIZA_INSTALL_DIR", s(&bin))])
        .fails(
            1,
            &[
                "error: Cannot download ",
                "Make sure that release v9.9.9 exists and has a binary for x86_64-unknown-linux-gnu.",
            ],
        );
    assert_eq!(m.server.take_hits(), downloads("v9.9.9", LINUX)[..1]);
    assert!(!bin.exists());

    for bad in ["latest", "../v0.1.0", "v0.1.0/x", "v0.1 .0", "v0.1.0?x=1"] {
        m.pipe(&[("TIZA_VERSION", bad), ("TIZA_INSTALL_DIR", s(&bin))])
            .fails(
                1,
                &[&format!("error: TIZA_VERSION={bad} is not a release tag.")],
            );
        assert_eq!(m.server.take_hits(), Vec::<String>::new(), "{bad}");
    }
}

#[test]
fn without_a_latest_release_it_tells_how_to_pin_one() {
    /// What releases/latest answers, from the base; `None` is a 404.
    type Latest = fn(&str) -> Option<Reply>;
    // GitHub today, with no release yet: releases/latest goes to releases.
    let cases: [(&str, Latest); 4] = [
        ("to-releases", |base| {
            Some(Reply::Redirect(base.to_string()))
        }),
        ("not-found", |_| None),
        ("no-redirect", |_| {
            Some(Reply::Body(b"<html></html>".to_vec()))
        }),
        ("bad-tag", |base| {
            Some(Reply::Redirect(format!("{base}/tag/nightly")))
        }),
    ];
    for (name, reply) in cases {
        let m = Machine::new(
            &format!("latest-{name}"),
            Platform::linux(),
            |work, base| {
                let mut routes = release(work, "v0.2.0");
                routes.extend(reply(base).map(|r| ("latest".to_string(), r)));
                routes
            },
        );
        let base = m.server.base.clone();
        let run = m.pipe(&[]);
        run.fails(
            1,
            &[
                &format!("error: Cannot find the latest release at {base}/latest.\n"),
                "Set TIZA_VERSION to a release tag, for example: TIZA_VERSION=v0.1.0\n",
            ],
        );
        // A page that is not there (404) is not an error: only the URL counts.
        assert!(!run.stderr.contains("curl:"), "{name}\n{run}");
        let hits = m.server.take_hits();
        assert_eq!(hits.first().map(String::as_str), Some("/releases/latest"));
        assert!(
            !hits.iter().any(|h| h.contains("/download/")),
            "{name}: {hits:?}"
        );
        assert!(!m.home.join(".local").exists(), "{name}");
    }
}

#[test]
fn a_curlrc_that_follows_redirects_still_finds_the_latest() {
    let m = Machine::new("curlrc", Platform::linux(), v020);
    std::fs::write(m.home.join(".curlrc"), "location\n").unwrap();
    let run = m.pipe(&[]);
    run.ok();
    assert!(
        run.stdout
            .contains("\nVersion: v0.2.0 (the latest release)\n"),
        "{run}"
    );
}

// --- AC5 ------------------------------------------------------------------

/// v020 with the reply of `file` (under download/v0.2.0/, NAME for the
/// Linux archive) replaced by `body`, or removed.
fn v020_with(work: &Path, base: &str, file: &str, body: Option<Vec<u8>>) -> Vec<(String, Reply)> {
    let path = format!(
        "download/v0.2.0/{}",
        file.replace("NAME", &format!("tiza-v0.2.0-{LINUX}"))
    );
    let mut routes: Vec<(String, Reply)> = v020(work, base)
        .into_iter()
        .filter(|(p, _)| *p != path)
        .collect();
    if let Some(body) = body {
        routes.push((path, Reply::Body(body)));
    }
    routes
}

/// v020 with `bytes` as the Linux archive, with a matching checksum.
fn v020_linux_archive(work: &Path, base: &str, bytes: Vec<u8>) -> Vec<(String, Reply)> {
    let sha = format!("{}  x\n", sha256(work, &bytes));
    let mut routes = v020_with(work, base, "NAME.tar.gz", Some(bytes));
    routes.retain(|(p, _)| !p.ends_with(".sha256") || !p.contains(LINUX));
    let path = format!("download/v0.2.0/tiza-v0.2.0-{LINUX}.tar.gz.sha256");
    routes.push((path, Reply::Body(sha.into_bytes())));
    routes
}

#[test]
fn a_checksum_that_does_not_match_installs_nothing() {
    let zeros = format!("{}  tiza-v0.2.0-{LINUX}.tar.gz\n", "0".repeat(64));
    let cases: [(&str, Vec<u8>); 4] = [
        ("other-hash", zeros.into_bytes()),
        ("not-a-hash", b"not a checksum\n".to_vec()),
        ("short", b"abc123  x.tar.gz\n".to_vec()),
        ("empty", Vec::new()),
    ];
    for (name, body) in cases {
        let m = Machine::new(&format!("sha-{name}"), Platform::linux(), |work, base| {
            v020_with(work, base, "NAME.tar.gz.sha256", Some(body))
        });
        let bin = m.dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tiza"), "old\n").unwrap();
        let run = m.pipe(&[("TIZA_INSTALL_DIR", s(&bin))]);
        run.fails(
            1,
            &[&format!(
                "error: The SHA-256 checksum of tiza-v0.2.0-{LINUX}.tar.gz does not match its .sha256 file.\nTiza is not installed.\n"
            )],
        );
        assert!(!run.stdout.contains("Installed:"), "{name}\n{run}");
        assert_eq!(
            std::fs::read_to_string(bin.join("tiza")).unwrap(),
            "old\n",
            "{name}"
        );
        assert_eq!(names(&bin), ["tiza"], "{name}");
        assert_eq!(
            names(&m.tmp),
            Vec::<String>::new(),
            "{name}: work folder left"
        );
    }
}

#[test]
fn a_missing_or_broken_download_installs_nothing() {
    let cases: [(&str, &str, Option<Vec<u8>>, &str); 3] = [
        ("no-archive", "NAME.tar.gz", None, "error: Cannot download "),
        (
            "no-checksum",
            "NAME.tar.gz.sha256",
            None,
            ".tar.gz.sha256.\n",
        ),
        (
            "not-a-tar",
            "NAME.tar.gz",
            Some(b"not a tar".to_vec()),
            "error: Cannot extract ",
        ),
    ];
    for (name, file, body, line) in cases {
        let m = Machine::new(
            &format!("download-{name}"),
            Platform::linux(),
            |work, base| match body {
                // A matching checksum, so only tar can refuse it.
                Some(bytes) => v020_linux_archive(work, base, bytes),
                None => v020_with(work, base, file, None),
            },
        );
        let bin = m.dir.join("bin");
        m.pipe(&[("TIZA_INSTALL_DIR", s(&bin))]).fails(1, &[line]);
        assert!(!bin.exists(), "{name}");
        assert_eq!(
            names(&m.tmp),
            Vec::<String>::new(),
            "{name}: work folder left"
        );
    }
}

#[test]
fn shasum_checks_when_there_is_no_sha256sum() {
    let m = Machine::new("shasum", Platform::linux(), v020);
    let no_sha256sum = path_without(&m.dir, "no-sha256sum", &["sha256sum"]);
    if no_sha256sum.join("shasum").exists() {
        let path = format!("{}:{}", s(&m.fake_bin), s(&no_sha256sum));
        let bin = m.dir.join("bin");
        let run = m.pipe(&[("TIZA_INSTALL_DIR", s(&bin)), ("PATH", &path)]);
        run.ok();
        assert!(run.stdout.contains("\nChecksum: OK (SHA-256)\n"), "{run}");
        assert_eq!(names(&bin), ["tiza"]);
    } else {
        eprintln!("shasum is not in PATH: the shasum install is skipped");
    }
    m.server.take_hits();

    let neither = path_without(&m.dir, "no-hasher", &["sha256sum", "shasum"]);
    let path = format!("{}:{}", s(&m.fake_bin), s(&neither));
    let bin = m.dir.join("bin-neither");
    m.pipe(&[("TIZA_INSTALL_DIR", s(&bin)), ("PATH", &path)])
        .fails(
            1,
            &["error: This script needs sha256sum or shasum to check the download.\n"],
        );
    assert_eq!(m.server.take_hits(), Vec::<String>::new());
    assert!(!bin.exists());
}

// --- AC6 ------------------------------------------------------------------

#[test]
fn tells_the_exact_line_when_the_folder_is_not_in_path() {
    let m = Machine::new("path", Platform::linux(), v020);
    let bin = m.dir.join("bin");
    let b = s(&bin);
    let home = s(&m.home);
    for (shell, rc) in [
        ("/bin/zsh", ".zshrc"),
        ("/usr/bin/bash", ".bashrc"),
        ("/bin/sh", ".profile"),
        ("/opt/x/ksh", ".profile"),
    ] {
        let run = m.pipe(&[("TIZA_INSTALL_DIR", b), ("SHELL", shell)]);
        run.ok();
        assert!(
            run.stdout.contains(&format!(
                "\n{b} is not in PATH. Add this line to {home}/{rc}, then open a new terminal:\n  export PATH=\"{b}:$PATH\"\n"
            )),
            "{shell}\n{run}"
        );
    }
    let run = m.pipe(&[("TIZA_INSTALL_DIR", b), ("SHELL", "/opt/homebrew/bin/fish")]);
    assert!(
        run.ok().stdout.contains(&format!(
            "\n{b} is not in PATH. Run this command in fish, then open a new terminal:\n  fish_add_path \"{b}\"\n"
        )),
        "{run}"
    );
    // bash on macOS reads .bash_profile.
    let mac = Machine::new("path-mac", Platform::of("aarch64-apple-darwin"), v020);
    let run = mac.pipe(&[("TIZA_INSTALL_DIR", b), ("SHELL", "/bin/bash")]);
    let mac_home = s(&mac.home);
    assert!(
        run.ok()
            .stdout
            .contains(&format!("Add this line to {mac_home}/.bash_profile,")),
        "{run}"
    );

    // In PATH, also as `DIR/` (dash finds `DIR//tiza` there): no notice
    // and no warning.
    let slash = PathBuf::from(format!("{b}/"));
    let shells: Vec<&str> = ["sh", "dash"].into_iter().filter(|sh| has(sh)).collect();
    for shell in shells {
        for front in [&bin, &slash] {
            let path = m.path(&[front]);
            let run = m.run(
                shell,
                true,
                &[],
                &[("TIZA_INSTALL_DIR", b), ("PATH", &path)],
            );
            run.ok();
            assert!(!run.stdout.contains("is not in PATH"), "{shell}\n{run}");
            assert_eq!(run.stderr, HTTP_WARNING, "{shell} {front:?}");
        }
    }

    // A folder name with a space, `$`, `"` and a backquote: the line to
    // copy gives the folder back.
    let odd = m.dir.join("my $HOME \"x\" `y`").join("bin");
    let run = m.pipe(&[("TIZA_INSTALL_DIR", s(&odd))]);
    run.ok();
    let line = run
        .stdout
        .lines()
        .find(|l| l.starts_with("  export PATH="))
        .unwrap_or_else(|| panic!("{run}"));
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!("{line}; printf %s \"$PATH\""))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("{}:/usr/bin:/bin", s(&odd)),
        "{line}"
    );
    let fish = s(&odd)
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$");
    let run = m.pipe(&[("TIZA_INSTALL_DIR", s(&odd)), ("SHELL", "/usr/bin/fish")]);
    assert!(
        run.ok()
            .stdout
            .contains(&format!("\n  fish_add_path \"{fish}\"\n")),
        "{run}"
    );

    // In PATH, after another tiza: a warning names it.
    let other = m.dir.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let other_tiza = other.join("tiza");
    std::fs::write(&other_tiza, "#!/bin/sh\necho other\n").unwrap();
    std::fs::set_permissions(&other_tiza, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = m.path(&[&other, &bin]);
    let run = m.pipe(&[("TIZA_INSTALL_DIR", b), ("PATH", &path)]);
    run.ok();
    assert!(
        run.stderr.contains(&format!(
            "warning: The command tiza runs {}, which comes before {b} in PATH.\n",
            s(&other_tiza)
        )),
        "{run}"
    );
}

#[test]
fn a_relative_install_dir_is_made_absolute() {
    let m = Machine::new("relative", Platform::linux(), v020);
    let run = m.pipe(&[("TIZA_INSTALL_DIR", "tools/bin")]);
    run.ok();
    let bin = m.dir.join("tools/bin");
    assert!(
        run.stdout
            .contains(&format!("\nInstalled: {}/tiza\n", s(&bin))),
        "{run}"
    );
    assert_eq!(names(&bin), ["tiza"]);
    // A name that starts with a dash is a folder, not an option.
    m.pipe(&[("TIZA_INSTALL_DIR", "-v")]).ok();
    assert_eq!(names(&m.dir.join("-v")), ["tiza"]);
}

#[test]
fn an_install_dir_that_cannot_take_the_binary_stops_before_any_download() {
    let m = Machine::new("bad-dir", Platform::linux(), v020);
    let bin = m.dir.join("bin");
    std::fs::create_dir_all(bin.join("tiza")).unwrap();
    m.pipe(&[("TIZA_INSTALL_DIR", s(&bin))]).fails(
        1,
        &[&format!(
            "error: {}/tiza is a folder. Remove it, or set TIZA_INSTALL_DIR to another folder.\n",
            s(&bin)
        )],
    );
    assert_eq!(m.server.take_hits(), Vec::<String>::new());

    m.pipe(&[("HOME", "")]).fails(
        1,
        &["error: HOME is not set. Set TIZA_INSTALL_DIR to the folder for the binary.\n"],
    );
    assert_eq!(m.server.take_hits(), Vec::<String>::new());

    // A file where a folder of the path must be.
    let plain = m.dir.join("plain");
    std::fs::write(&plain, "").unwrap();
    m.pipe(&[("TIZA_INSTALL_DIR", s(&plain.join("bin")))])
        .fails(
            1,
            &[&format!(
                "error: {} is not a folder. Set TIZA_INSTALL_DIR to a folder.\n",
                s(&plain)
            )],
        );
    assert_eq!(m.server.take_hits(), Vec::<String>::new());

    // A folder of another user, or under one: root writes in any folder, so
    // skip as root.
    let locked = m.dir.join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(locked.join("probe"), "").is_ok() {
        eprintln!("this user writes in a 0555 folder (root): the locked folder is skipped");
    } else {
        for dir in [locked.clone(), locked.join("sub/bin")] {
            m.pipe(&[("TIZA_INSTALL_DIR", s(&dir))]).fails(
                1,
                &[&format!(
                    "error: You cannot write in {}. Set TIZA_INSTALL_DIR to a folder of your user.\n",
                    s(&locked)
                )],
            );
            assert_eq!(m.server.take_hits(), Vec::<String>::new(), "{dir:?}");
        }
        assert_eq!(names(&locked), Vec::<String>::new());
    }
    assert_eq!(names(&m.tmp), Vec::<String>::new(), "work folder left");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_new_binary_that_does_not_start_leaves_the_old_one() {
    let m = Machine::new("broken", Platform::linux(), |work, base| {
        let tiza = "#!/bin/sh\necho 'tiza: GLIBC_2.35 not found' >&2\nexit 1\n";
        let bytes = pack(work, "v0.2.0", LINUX, tiza);
        v020_linux_archive(work, base, bytes)
    });
    let bin = m.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("tiza"), "old\n").unwrap();
    let run = m.pipe(&[("TIZA_INSTALL_DIR", s(&bin))]);
    run.fails(
        1,
        &[&format!(
            "error: The tiza binary of tiza-v0.2.0-{LINUX} does not start on this machine:\n\
             tiza: GLIBC_2.35 not found\nTiza is not installed.\n"
        )],
    );
    assert!(!run.stdout.contains("Installed:"), "{run}");
    assert_eq!(std::fs::read_to_string(bin.join("tiza")).unwrap(), "old\n");
    assert_eq!(names(&bin), ["tiza"], "no staged binary left");
    assert_eq!(names(&m.tmp), Vec::<String>::new());
}

// --- AC7 ------------------------------------------------------------------

#[test]
fn the_work_folder_goes_away_after_a_signal() {
    let (arrived_tx, arrived_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let m = Machine::new("signal", Platform::linux(), |work, base| {
        let path = format!("download/v0.2.0/tiza-v0.2.0-{LINUX}.tar.gz");
        let mut routes: Vec<(String, Reply)> = v020(work, base)
            .into_iter()
            .filter(|(p, _)| *p != path)
            .collect();
        routes.push((path, Reply::Hold(arrived_tx, release_rx)));
        routes
    });
    let bin = m.dir.join("bin");
    let child = m.spawn("sh", false, &[], &[("TIZA_INSTALL_DIR", s(&bin))], b"");
    arrived_rx
        .recv_timeout(DEADLINE)
        .expect("the download did not start");
    let work = names(&m.tmp);
    assert!(
        work.len() == 1 && work[0].starts_with("tiza-install."),
        "work folder during the download: {work:?}"
    );
    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    // The shell runs its trap when the download ends: end it now.
    release_tx.send(()).unwrap();
    let run = finish(child);
    assert_eq!(run.code, Some(143), "{run}");
    assert_eq!(
        names(&m.tmp),
        Vec::<String>::new(),
        "work folder left\n{run}"
    );
    assert!(!bin.exists());
}

#[test]
fn wget_downloads_when_there_is_no_curl() {
    let m = Machine::new("wget", Platform::linux(), v020);
    let bin = m.dir.join("bin");
    let no_curl = path_without(&m.dir, "no-curl", &["curl"]);
    if let Ok(real) = no_curl.join("wget").read_link() {
        // The real wget behind a script that writes down its arguments.
        let log = m.dir.join("wget.log");
        std::fs::remove_file(no_curl.join("wget")).unwrap();
        std::fs::write(
            no_curl.join("wget"),
            format!(
                "#!/bin/sh\necho \"$*\" >> '{}'\nexec '{}' \"$@\"\n",
                s(&log),
                s(&real)
            ),
        )
        .unwrap();
        std::fs::set_permissions(no_curl.join("wget"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let path = format!("{}:{}", s(&m.fake_bin), s(&no_curl));
        let run = m.pipe(&[("TIZA_INSTALL_DIR", s(&bin)), ("PATH", &path)]);
        run.ok();
        assert!(
            run.stdout
                .contains("\ntiza 0.2.0 (fake x86_64-unknown-linux-gnu)\n"),
            "{run}"
        );
        let mut hits = vec!["/releases/latest".to_string()];
        hits.extend(downloads("v0.2.0", LINUX));
        assert_eq!(m.server.take_hits(), hits);
        assert_eq!(names(&m.tmp), Vec::<String>::new());
        // Each request has the options that this wget knows.
        let help = Command::new(&real).arg("--help").output().unwrap();
        let help = String::from_utf8_lossy(&help.stdout).into_owned();
        let log = std::fs::read_to_string(&log).unwrap();
        let requests: Vec<&str> = log.lines().filter(|l| *l != "--help").collect();
        assert_eq!(requests.len(), 3, "{log}");
        for (option, known) in [
            ("--no-hsts", "--no-hsts"),
            ("--secure-protocol=TLSv1_2", "--secure-protocol"),
            ("--tries=3 --timeout=30", "--tries"),
        ] {
            for request in &requests {
                assert_eq!(request.contains(option), help.contains(known), "{request}");
            }
        }
    } else {
        eprintln!("wget is not in PATH: the wget install is skipped");
    }

    let neither = path_without(&m.dir, "no-curl-no-wget", &["curl", "wget"]);
    let path = format!("{}:{}", s(&m.fake_bin), s(&neither));
    let bin = m.dir.join("bin-neither");
    m.pipe(&[("TIZA_INSTALL_DIR", s(&bin)), ("PATH", &path)]).fails(
        1,
        &["error: This script needs curl or wget. Install one of them, then run the script again.\n"],
    );
    assert_eq!(m.server.take_hits(), Vec::<String>::new());
    assert!(!bin.exists());
}

#[test]
fn the_script_names_the_release_files_and_keeps_to_https() {
    let script = read("install.sh");
    assert!(script.starts_with("#!/bin/sh\n"));
    assert_eq!(
        script.lines().rfind(|l| !l.trim().is_empty()),
        Some("main \"$@\""),
        "everything runs on the last line"
    );
    assert!(script.contains("\nREPO=luiscarlosjayk/tiza\n"));
    assert!(script.contains("base=${TIZA_DOWNLOAD_BASE:-https://github.com/$REPO/releases}"));
    assert!(script.contains("https://*) proto='=https' ;;"));
    // Each curl call (not the usage lines) keeps to the protocol and TLS 1.2.
    let calls: Vec<&str> = script
        .lines()
        .map(str::trim_start)
        .filter(|l| (l.starts_with("curl ") || l.contains("$(curl ")) && !l.contains("SCRIPT_URL"))
        .collect();
    assert_eq!(calls.len(), 2, "{calls:#?}");
    for call in calls {
        assert!(
            call.contains("curl --proto \"$proto\" --tlsv1.2 "),
            "{call}"
        );
    }
    for word in ["sudo", "api.github.com"] {
        assert!(!script.contains(word), "{word}");
    }
    // The shell writes the downloads: the curl snap cannot write in /tmp.
    assert!(script.contains("curl --proto \"$proto\" --tlsv1.2 -fsSL \"$1\" >\"$2\"\n"));
    assert!(script.contains("wget_run -q -O - \"$1\" >\"$2\"\n"));

    // The names of the package step of release.yml.
    assert!(script.contains("name=tiza-$tag-$target\n"));
    assert!(script.contains("url=$base/download/$tag/$name.tar.gz\n"));
    assert!(script.contains("fetch \"$url.sha256\" \"$tmp/$name.tar.gz.sha256\""));
    let workflow = read(".github/workflows/release.yml");
    for line in [
        "name=\"tiza-${GITHUB_REF_NAME}-${{ matrix.target }}\"",
        "cp \"target/${{ matrix.target }}/release/tiza\" \"dist/$name/\"",
        "tar -C dist -czf \"dist/$name.tar.gz\" \"$name\"",
        "shasum -a 256 \"$name.tar.gz\" > \"$name.tar.gz.sha256\"",
    ] {
        assert!(workflow.contains(line), "release.yml: {line}");
    }
    let mut built: Vec<&str> = workflow
        .lines()
        .filter_map(|l| l.split("target: ").nth(1))
        .map(|t| t.trim_end_matches([' ', '}']))
        .collect();
    built.sort_unstable();
    let mut known: Vec<&str> = TARGETS.iter().map(|t| t.2).collect();
    known.sort_unstable();
    assert_eq!(built, known, "the targets of release.yml");

    assert!(read("CHANGELOG.md").contains("`install.sh`"));
}

// --- AC8 ------------------------------------------------------------------

#[test]
fn help_lists_the_variables() {
    let m = Machine::new("help", Platform::new("FreeBSD", "amd64"), v020);
    for (pipe, flag) in [
        (true, "--help"),
        (false, "--help"),
        (true, "-h"),
        (false, "-h"),
    ] {
        let run = m.run("sh", pipe, &[flag], &[]);
        run.ok();
        for line in [
            "  curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh\n",
            "  TIZA_VERSION      The release tag to install, for example v0.1.0.\n",
            "  TIZA_INSTALL_DIR  The folder for the binary.\n",
            "The default is $HOME/.local/bin.\n",
            "cargo install tiza",
        ] {
            assert!(run.stdout.contains(line), "{flag}: {line:?}\n{run}");
        }
        assert!(!run.stdout.contains("TIZA_DOWNLOAD_BASE"), "for tests only");
    }
    let run = m.run("sh", false, &["--nope"], &[]);
    run.fails(2, &["error: unknown option: --nope\n"]);
    assert!(run.stderr.contains("TIZA_INSTALL_DIR"), "{run}");
    assert_eq!(run.stdout, "");
    assert_eq!(m.server.take_hits(), Vec::<String>::new());
}

// --- AC9 ------------------------------------------------------------------

#[test]
fn the_readme_gives_the_command_first() {
    let command =
        "curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh";
    let script = read("install.sh");
    assert!(
        script.contains("SCRIPT_URL=https://raw.githubusercontent.com/$REPO/main/install.sh\n")
    );
    let readme = read("README.md");
    let install = readme
        .split("\n## Installation\n")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("an Installation section");
    let first = install
        .find(&format!("```sh\n{command}\n```"))
        .expect("the command");
    let at = install
        .find("cargo install tiza")
        .expect("cargo install tiza");
    assert!(first < at, "the command comes before cargo install tiza");
    // The install page gives the options of the script; the variable that
    // only the tests use is in neither.
    let page = read("docs/guides/install.md");
    for text in [install, page.as_str()] {
        assert!(!text.contains("TIZA_DOWNLOAD_BASE"), "for tests only");
    }
}
