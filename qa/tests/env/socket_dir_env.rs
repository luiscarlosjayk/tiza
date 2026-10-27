//! Empty env vars are unset.
// `set_var` is unsafe in Rust 2024: the only unsafe code of tiza-qa.
#![allow(unsafe_code)]

#[test]
fn empty_xdg_runtime_dir_is_ignored() {
    // Env vars are process-wide: this test runs in a binary of its own,
    // one test at a time.
    let _env = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let keys = ["XDG_RUNTIME_DIR", "TMPDIR"];
    let saved: Vec<_> = keys.iter().map(std::env::var_os).collect();
    // SAFETY: each test of this crate holds ENV_LOCK while it changes or
    // reads the environment, so no other thread uses it now.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", "");
        std::env::set_var("TMPDIR", "/tmp/qa-socket-env");
    }
    let p = tiza_core::socket::resolve(None, None);
    assert!(p.starts_with("/tmp/qa-socket-env/tiza"), "{}", p.display());
    // SAFETY: as above, no other thread uses the environment.
    unsafe {
        std::env::set_var("TMPDIR", "");
    }
    let p = tiza_core::socket::resolve(None, None);
    assert!(p.starts_with("/tmp/tiza"), "{}", p.display());
    // The next test of the crate gets the environment back.
    // SAFETY: as above.
    unsafe {
        for (k, v) in keys.iter().zip(&saved) {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}
