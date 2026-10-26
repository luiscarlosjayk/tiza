//! Empty env vars are unset.
// `set_var` is unsafe in Rust 2024: the only unsafe code of tiza-qa.
#![allow(unsafe_code)]

#[test]
fn empty_xdg_runtime_dir_is_ignored() {
    // Env vars are process-wide: this test runs in its own binary.
    // SAFETY: the `env` crate has this test only, and nothing in it reads
    // or writes the environment on another thread while it runs.
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
}
