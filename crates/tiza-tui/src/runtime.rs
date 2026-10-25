//! The folders that hold Tiza's sockets and `panics.log`.
//!
//! Tiza's own folder, `tiza/` in `$XDG_RUNTIME_DIR`, `$TMPDIR` or `/tmp`
//! ([`tiza_core::socket::runtime_dir`]), is made with 0700. When it is
//! there already, it must be a folder (not a link) of this user: another
//! user's folder is refused, and wider permissions become 0700. A socket
//! path that the user gives (`--socket`, `TIZA_SOCKET`) is in a folder that
//! is not Tiza's: Tiza makes the folders that are missing, each with 0700,
//! and does not change a folder that is there (`$HOME`, the current
//! folder).

use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;

/// Makes the folder of `path` (a socket or `panics.log`) ready, by the rule
/// of its kind (see the module).
pub fn prepare_parent(path: &Path) -> io::Result<()> {
    let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) else {
        return Ok(());
    };
    if dir == tiza_core::socket::runtime_dir() {
        own_folder(dir)
    } else {
        make_missing(dir)
    }
}

/// Tiza's own folder: made with 0700 when it is missing, then checked.
fn own_folder(dir: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(dir) {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(base) = dir.parent() {
                make_missing(base)?;
            }
            match std::fs::DirBuilder::new().mode(0o700).create(dir) {
                // Another process made it meanwhile: it is checked below.
                Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
                _ => {}
            }
        }
        Err(e) => return Err(e),
    }
    let meta = std::fs::symlink_metadata(dir)?;
    check_own(dir, &meta, rustix::process::geteuid().as_raw())?;
    if meta.mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("cannot make {} private (0700): {e}", dir.display()),
            )
        })?;
    }
    Ok(())
}

/// Whether Tiza may use its own folder `dir`, from the metadata of the
/// path itself (a link is not followed): it must be a folder of the user
/// `euid`. Public for the tests, which have no second user to own a folder.
pub fn check_own(dir: &Path, meta: &std::fs::Metadata, euid: u32) -> io::Result<()> {
    if !meta.is_dir() {
        return Err(refused(dir, "it is not a folder"));
    }
    if meta.uid() != euid {
        return Err(refused(dir, "it belongs to another user"));
    }
    Ok(())
}

fn refused(dir: &Path, why: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "Tiza does not use {}: {why}. Set XDG_RUNTIME_DIR or TMPDIR to a folder of yours, or give a socket path with --socket",
            dir.display()
        ),
    )
}

/// A folder that is not Tiza's: makes the folders that are missing, each
/// with 0700. A folder that is there stays as it is.
fn make_missing(dir: &Path) -> io::Result<()> {
    let mut missing = Vec::new();
    let mut next = Some(dir);
    while let Some(d) = next {
        if d.as_os_str().is_empty() || std::fs::symlink_metadata(d).is_ok() {
            break;
        }
        missing.push(d);
        next = d.parent();
    }
    for d in missing.into_iter().rev() {
        match std::fs::DirBuilder::new().mode(0o700).create(d) {
            Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
            _ => {}
        }
    }
    Ok(())
}
