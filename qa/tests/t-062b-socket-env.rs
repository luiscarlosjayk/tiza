//! t-062 (follow-up from the QA agent) — empty env vars are unset.

#[test]
fn empty_xdg_runtime_dir_is_ignored() {
    // Env vars are process-wide: this test runs in its own binary.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", "");
        std::env::set_var("TMPDIR", "/tmp/qa-t062");
    }
    let p = tiza_core::socket::resolve(None, None);
    assert!(p.starts_with("/tmp/qa-t062/tiza"), "{}", p.display());
    unsafe {
        std::env::set_var("TMPDIR", "");
    }
    let p = tiza_core::socket::resolve(None, None);
    assert!(p.starts_with("/tmp/tiza"), "{}", p.display());
}
