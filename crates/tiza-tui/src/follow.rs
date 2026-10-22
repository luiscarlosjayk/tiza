//! Follow mode: reload the code when the source file changes on disk
//! (Zed saves with tmp + rename, so the *parent directory* is watched), with
//! a debounce, keeping the annotations by re-anchoring them to their text.

use std::path::{Path, PathBuf};
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
    watcher: Option<notify::RecommendedWatcher>,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
    /// Time of the last relevant event, while a reload is pending.
    pending_since: Option<Instant>,
    /// An empty read was deferred once, waiting for the editor's second write.
    deferred_empty: bool,
    pub reloads: u64,
    pub last_reload: Option<Instant>,
    pub last_reload_clock: Option<String>,
    /// Content as last loaded, to skip reloads that change nothing.
    pub last_content: Vec<String>,
    /// Whether the file is currently missing on disk.
    pub missing: bool,
}

impl Follow {
    /// Watches the parent directory of `path`.
    pub fn start(path: PathBuf, current: Vec<String>) -> Result<Follow, String> {
        let dir = path
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .map_err(|e| format!("cannot watch {}: {e}", dir.display()))?;
        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(|e| format!("cannot watch {}: {e}", dir.display()))?;
        let dir_canon = canonical_dir(&dir);
        Ok(Follow {
            path,
            dir_canon,
            watcher: Some(watcher),
            rx,
            pending_since: None,
            deferred_empty: false,
            reloads: 0,
            last_reload: None,
            last_reload_clock: None,
            last_content: current,
            missing: false,
        })
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    /// Drains the watcher's events. Returns true when a reload should be
    /// attempted now (an event touched the file and the debounce elapsed).
    pub fn poll(&mut self) -> bool {
        let mut touched = false;
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
                true
            }
            _ => false,
        }
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

    /// Stops watching (dropping the watcher).
    pub fn stop(&mut self) {
        self.watcher = None;
    }
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

        let through_link = Follow::start(link.join("main.rs"), vec![]).unwrap();
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

        let through_real = Follow::start(real.join("main.rs"), vec![]).unwrap();
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
