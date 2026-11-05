//! Resolution of the local API's Unix socket path (same order as herdr):
//! explicit `--socket` > `$TIZA_SOCKET` > session (`--session` or `$TIZA_SESSION`)
//! inside `$XDG_RUNTIME_DIR` or `$TMPDIR` or `/tmp`, as `tiza/<session>.sock`.

use std::path::{Path, PathBuf};

pub const ENV_SOCKET: &str = "TIZA_SOCKET";
pub const ENV_SESSION: &str = "TIZA_SESSION";

pub fn runtime_dir() -> PathBuf {
    // An empty variable counts as unset (a cwd-relative `tiza/` would be a surprise).
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var_os("TMPDIR").filter(|v| !v.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("tiza")
}

pub fn resolve(explicit: Option<&Path>, session: Option<&str>) -> PathBuf {
    resolve_with_origin(explicit, session).0
}

/// Where the socket path of [`resolve_with_origin`] came from: a message
/// that names the way out (`--session NAME` or `--socket PATH`) follows it,
/// because `--socket` and `$TIZA_SOCKET` win over a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// `--socket PATH`.
    Flag,
    /// `$TIZA_SOCKET`.
    EnvSocket,
    /// `--session NAME` or `$TIZA_SESSION`, as given.
    Session(String),
    /// No socket and no session: the session `default`.
    Default,
}

/// [`resolve`], and where the path came from.
pub fn resolve_with_origin(explicit: Option<&Path>, session: Option<&str>) -> (PathBuf, Origin) {
    if let Some(p) = explicit {
        return (p.to_path_buf(), Origin::Flag);
    }
    if let Some(p) = std::env::var_os(ENV_SOCKET)
        && !p.is_empty()
    {
        return (PathBuf::from(p), Origin::EnvSocket);
    }
    let (session, origin) = match session
        .map(|s| s.to_string())
        .or_else(|| std::env::var(ENV_SESSION).ok().filter(|s| !s.is_empty()))
    {
        Some(s) => (s.clone(), Origin::Session(s)),
        None => ("default".to_string(), Origin::Default),
    };
    (
        runtime_dir().join(format!("{}.sock", sanitize(&session))),
        origin,
    )
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_wins() {
        assert_eq!(
            resolve(Some(Path::new("/x/y.sock")), Some("s")),
            PathBuf::from("/x/y.sock")
        );
    }

    #[test]
    fn session_name_is_sanitized() {
        let p = resolve(None, Some("mi sesión/rara"));
        assert!(p.to_string_lossy().ends_with("tiza/mi_sesi_n_rara.sock"));
    }
}
