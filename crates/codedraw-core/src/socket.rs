//! Resolution of the local API's Unix socket path (same order as herdr):
//! explicit `--socket` > `$CODEDRAW_SOCKET` > session (`--session` or `$CODEDRAW_SESSION`)
//! inside `$XDG_RUNTIME_DIR` or `$TMPDIR` or `/tmp`, as `codedraw/<session>.sock`.

use std::path::{Path, PathBuf};

pub const ENV_SOCKET: &str = "CODEDRAW_SOCKET";
pub const ENV_SESSION: &str = "CODEDRAW_SESSION";

pub fn runtime_dir() -> PathBuf {
    // An empty variable counts as unset (a cwd-relative `codedraw/` would be a surprise).
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var_os("TMPDIR").filter(|v| !v.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("codedraw")
}

pub fn resolve(explicit: Option<&Path>, session: Option<&str>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Some(p) = std::env::var_os(ENV_SOCKET)
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    let session = session
        .map(|s| s.to_string())
        .or_else(|| std::env::var(ENV_SESSION).ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "default".into());
    runtime_dir().join(format!("{}.sock", sanitize(&session)))
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

/// Creates the socket directory with 0700 permissions (owner only).
pub fn prepare_dir(path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    Ok(())
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
        assert!(
            p.to_string_lossy()
                .ends_with("codedraw/mi_sesi_n_rara.sock")
        );
    }
}
