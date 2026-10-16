//! Resolución de la ruta del socket Unix de la API local (mismo orden que herdr):
//! `--socket` explícito > `$CODEDRAW_SOCKET` > sesión (`--session` o `$CODEDRAW_SESSION`)
//! dentro de `$XDG_RUNTIME_DIR` o `$TMPDIR` o `/tmp`, como `codedraw/<sesión>.sock`.

use std::path::{Path, PathBuf};

pub const ENV_SOCKET: &str = "CODEDRAW_SOCKET";
pub const ENV_SESSION: &str = "CODEDRAW_SESSION";

pub fn runtime_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .or_else(|| std::env::var_os("TMPDIR"))
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

/// Crea el directorio del socket con permisos 0700 (solo el usuario).
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
