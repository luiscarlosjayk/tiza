//! Follow mode: reload the code when the source file changes on disk
//! (Zed saves with tmp + rename, so the *parent directory* is watched), with
//! a debounce, keeping the annotations by re-anchoring them to their text.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

/// Minimum quiet time after the last event before reloading.
pub const DEBOUNCE: Duration = Duration::from_millis(120);

pub struct Follow {
    /// Absolute path of the followed file, as given (shown in the status line).
    pub path: PathBuf,
    /// Canonical parent directory (symlinks resolved). FSEvents on macOS
    /// reports `/private/var/…` for a watch on `/var/…` (`$TMPDIR`), so event
    /// paths are compared against this, never against `path.parent()`.
    dir_canon: PathBuf,
    watch: Watch,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    /// Time of the last relevant event, while a reload is pending.
    pending_since: Option<Instant>,
    /// An empty read was deferred once, waiting for the editor's second write.
    deferred_empty: bool,
    pub reloads: u64,
    /// The file on disk at the last read to load it, taken just before the
    /// read. A test compares it with the file after a save: when they are
    /// the same, the read saw that save.
    pub last_read: Option<Stamp>,
    /// How many times the file was read to load it: after a save (once the
    /// debounce elapsed), when follow starts and at the resume step. The
    /// reads that load nothing count too. Two saves in one debounce are
    /// one read.
    pub reads: u64,
    /// The sha256 of the bytes of the last read, in lowercase hex. `None`
    /// before a read, or when Tiza could not read the file (for example
    /// when it is removed); a file that is not UTF-8 has one. The lines
    /// come from the same bytes.
    pub hash: Option<String>,
    pub last_reload: Option<Instant>,
    pub last_reload_clock: Option<String>,
    /// Content as last loaded, to skip reloads that change nothing.
    pub last_content: Vec<String>,
    /// Whether the file is currently missing on disk.
    pub missing: bool,
}

/// What tells one save of a file from the next: its size, its time of
/// change and its inode (a save with a temp file and a rename makes a new
/// one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub len: u64,
    pub modified: Option<std::time::SystemTime>,
    pub inode: u64,
}

/// The sha256 of `bytes`, in lowercase hex: what `shasum -a 256` prints.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Stamp {
    /// The stamp of the file at `path` now. `None` when it cannot be read.
    pub fn of(path: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(path).ok()?;
        Some(Stamp {
            len: m.len(),
            modified: m.modified().ok(),
            inode: m.ino(),
        })
    }
}

/// The file watcher. It starts and stops on threads of its own: with
/// fseventsd busy (a build running beside Tiza) starting the event stream
/// and dropping it each took seconds, and the TUI froze meanwhile.
enum Watch {
    /// Starting on its own thread, which sends it here (or why it failed).
    Starting(mpsc::Receiver<Result<Counted, String>>),
    Running(Counted),
    /// No watcher: a follow of the tests that read the file by hand
    /// ([`Follow::by_hand`]).
    #[cfg(any(test, feature = "test-hooks"))]
    ByHand,
    Stopped,
}

/// What [`Follow::poll`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum Poll {
    /// Nothing to do now.
    Idle,
    /// The file changed and the debounce elapsed: reload now.
    Reload,
    /// The watcher has just started: the file may have changed while it
    /// started (see [`Follow::touch`]).
    Started,
    /// The watcher could not start: follow is off, for this reason.
    Failed(String),
}

/// Watchers in this process, from the start of the thread that makes one
/// until its drop has ended (the tests wait on it, see `follow.status`).
static LIVE: AtomicUsize = AtomicUsize::new(0);

pub fn live_watchers() -> usize {
    LIVE.load(Ordering::SeqCst)
}

/// A watcher counted in [`LIVE`]: from [`Counted::begin`], before it
/// exists, until it has been dropped.
struct Counted(Option<notify::RecommendedWatcher>);

impl Counted {
    fn begin() -> Counted {
        LIVE.fetch_add(1, Ordering::SeqCst);
        Counted(None)
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        drop(self.0.take());
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Follow {
    /// Watches the parent directory of `path`. The watcher starts on a
    /// thread of its own and [`Follow::poll`] takes it when it is ready;
    /// a start that fails says so there.
    pub fn start(path: PathBuf, current: Vec<String>) -> Result<Follow, String> {
        let dir = parent_dir(&path);
        let (tx, rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let d = dir.clone();
        std::thread::Builder::new()
            .name("tiza follow start".into())
            .spawn(move || {
                let mut counted = Counted::begin();
                hook_slow();
                let cannot =
                    |e: &dyn std::fmt::Display| format!("cannot watch {}: {e}", d.display());
                let started = match notify::recommended_watcher(move |res| {
                    let _ = tx.send(res);
                }) {
                    Err(e) => Err(cannot(&e)),
                    Ok(_) if hook("follow:fail") => Err(cannot(&"test hook")),
                    Ok(w) => counted
                        .0
                        .insert(w)
                        .watch(&d, RecursiveMode::NonRecursive)
                        .map_err(|e| cannot(&e)),
                };
                // Stopped meanwhile: nobody takes it, and it is dropped
                // here, on this thread (so is a watcher that failed).
                let _ = started_tx.send(started.map(|()| counted));
            })
            .map_err(|e| format!("cannot watch {}: {e}", dir.display()))?;
        Ok(Follow::new(path, Watch::Starting(started_rx), rx, current))
    }

    /// Test hook, only in the builds of the tests (the `test-hooks`
    /// feature, and the unit tests): a follow with no watcher, so no file
    /// event and no read when it starts. The test reads the file with
    /// `App::read_save`; a `defer` still reads it again after the debounce.
    /// A watcher puts load on fseventsd for every process of the machine,
    /// and these tests need none of its events.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn by_hand(path: PathBuf, current: Vec<String>) -> Follow {
        // No sender: the channel of the events is closed from the start,
        // and `poll` reads a closed channel as one with no event. It is
        // not a watcher that ended.
        let (_, rx) = mpsc::channel();
        Follow::new(path, Watch::ByHand, rx, current)
    }

    fn new(
        path: PathBuf,
        watch: Watch,
        rx: mpsc::Receiver<notify::Result<notify::Event>>,
        current: Vec<String>,
    ) -> Follow {
        let dir_canon = canonical_dir(&parent_dir(&path));
        Follow {
            path,
            dir_canon,
            watch,
            rx,
            pending_since: None,
            deferred_empty: false,
            reloads: 0,
            last_read: None,
            reads: 0,
            hash: None,
            last_reload: None,
            last_reload_clock: None,
            last_content: current,
            missing: false,
        }
    }

    /// Whether the watcher is still starting on its own thread: until
    /// [`Follow::poll`] takes it.
    pub fn starting(&self) -> bool {
        matches!(self.watch, Watch::Starting(_))
    }

    /// Test hook, only in the builds of the tests (the `test-hooks`
    /// feature, and the unit tests): whether a read of the file is due
    /// after the debounce (a save seen, a `touch` or a `defer`).
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn reload_due(&self) -> bool {
        self.pending_since.is_some()
    }

    /// Test hook, only in the builds of the tests (the `test-hooks`
    /// feature, and the unit tests): what watches the
    /// file. `by_hand` for [`Follow::by_hand`], otherwise `starting`,
    /// `running` or `stopped`.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn watching(&self) -> &'static str {
        match self.watch {
            Watch::Starting(_) => "starting",
            Watch::Running(_) => "running",
            Watch::ByHand => "by_hand",
            Watch::Stopped => "stopped",
        }
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// Takes the watcher once it has started ([`Poll::Started`]) and
    /// drains its events: a reload when an event touched the file and the
    /// debounce elapsed.
    pub fn poll(&mut self) -> Poll {
        let mut touched = false;
        if let Watch::Starting(started) = &self.watch {
            match started.try_recv() {
                Ok(Ok(w)) => {
                    self.watch = Watch::Running(w);
                    return Poll::Started;
                }
                Ok(Err(why)) => {
                    self.watch = Watch::Stopped;
                    return Poll::Failed(why);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.watch = Watch::Stopped;
                    return Poll::Failed(self.did_not_start());
                }
            }
        }
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ok(ev) => {
                    if ev.paths.iter().any(|p| self.same_file(p)) {
                        touched = true;
                    }
                }
                Err(_) => touched = true,
            }
        }
        if touched {
            self.pending_since = Some(Instant::now());
        }
        match self.pending_since {
            Some(t) if t.elapsed() >= DEBOUNCE => {
                self.pending_since = None;
                Poll::Reload
            }
            _ => Poll::Idle,
        }
    }

    /// The watcher's thread ended without sending it (it panicked).
    fn did_not_start(&self) -> String {
        let dir = self.path.parent().unwrap_or(Path::new("."));
        format!("cannot watch {}: the watcher did not start", dir.display())
    }

    /// Reads the file again after the debounce, as for a save: what changed
    /// while the watcher started.
    pub fn touch(&mut self) {
        self.pending_since = Some(Instant::now());
    }

    /// A pending reload was deferred (empty read): try again after the debounce.
    pub fn defer(&mut self) {
        self.deferred_empty = true;
        self.pending_since = Some(Instant::now());
    }

    pub fn take_deferred(&mut self) -> bool {
        std::mem::take(&mut self.deferred_empty)
    }

    pub fn mark_reloaded(&mut self, content: Vec<String>) {
        self.reloads += 1;
        self.last_reload = Some(Instant::now());
        self.last_reload_clock = Some(clock_now());
        self.last_content = content;
        self.missing = false;
    }

    /// Whether an event path names the followed file: same file name and the
    /// same directory once symlinks are resolved on both sides.
    pub fn same_file(&self, p: &Path) -> bool {
        if p == self.path {
            return true;
        }
        if p.file_name() != self.path.file_name() {
            return false;
        }
        p.parent().is_some_and(|d| {
            let d = if d.as_os_str().is_empty() {
                Path::new(".")
            } else {
                d
            };
            canonical_dir(d) == self.dir_canon
        })
    }

    /// Stops watching. A running watcher is dropped on a thread of its own
    /// (dropping it waits for its event thread); one still starting is
    /// dropped by the thread that starts it.
    pub fn stop(&mut self) {
        if let Watch::Running(w) = std::mem::replace(&mut self.watch, Watch::Stopped) {
            // If the thread cannot start, the watcher is dropped here.
            let _ = std::thread::Builder::new()
                .name("tiza follow stop".into())
                .spawn(move || {
                    hook_slow();
                    drop(w);
                });
        }
    }
}

impl Drop for Follow {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Test hooks, only with the `test-hooks` feature ([`crate::test_hook`]).
/// `follow:slow` holds the start and the stop of the watcher 3 s each, as
/// a busy fseventsd does; `follow:fail` makes the start fail.
#[allow(unused_variables)]
fn hook(place: &str) -> bool {
    #[cfg(feature = "test-hooks")]
    if crate::test_hook(place) {
        return true;
    }
    false
}

fn hook_slow() {
    if hook("follow:slow") {
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// The directory of `path`, `.` for a bare file name.
fn parent_dir(path: &Path) -> PathBuf {
    path.parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The directory with symlinks resolved; falls back to the absolute form
/// when it cannot be resolved (removed while the editor replaced it).
fn canonical_dir(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir)
        .or_else(|_| std::path::absolute(dir))
        .unwrap_or_else(|_| dir.to_path_buf())
}

/// `HH:MM:SS` local time, for the status line.
pub fn clock_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Local offset without pulling a date-time crate: use TZ via libc-free
    // approximation (UTC) — the status line says so.
    let s = secs % 86_400;
    format!("{:02}:{:02}:{:02}Z", s / 3600, (s % 3600) / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// macOS: `$TMPDIR` is `/var/folders/…` but FSEvents reports
    /// `/private/var/folders/…`. Reproduced on any OS with a symlinked
    /// directory: the file is followed through the link, the event names
    /// the real path (and the other way round).
    #[test]
    fn same_file_resolves_symlinked_directories() {
        let base = std::env::temp_dir().join(format!("tiza-follow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = base.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(not(unix))]
        return;
        std::fs::write(real.join("main.rs"), "fn main() {}\n").unwrap();

        let through_link = Follow::by_hand(link.join("main.rs"), vec![]);
        assert!(
            through_link.same_file(&real.join("main.rs")),
            "real path for a linked watch"
        );
        assert!(through_link.same_file(&link.join("main.rs")));
        assert!(!through_link.same_file(&real.join("other.rs")));
        assert!(
            !through_link.same_file(&base.join("main.rs")),
            "same name, other directory"
        );

        let through_real = Follow::by_hand(real.join("main.rs"), vec![]);
        assert!(
            through_real.same_file(&link.join("main.rs")),
            "linked path for a real watch"
        );

        // A relative event path (some backends) still matches by directory.
        let cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&real).unwrap();
        assert!(through_real.same_file(Path::new("main.rs")));
        std::env::set_current_dir(cwd).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }
}
