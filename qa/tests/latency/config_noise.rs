//! A config file of noise (1 MB of random bytes, most of them not UTF-8)
//! does not make Tiza slow to start: the lines of the bad bytes are counted
//! once, and the recovery stops after MAX_ERRORS passes.

use std::time::Instant;

#[test]
fn a_config_of_one_megabyte_of_noise_is_read_in_time() {
    let _alone = crate::alone();
    let dir = tiza_qa::scratch("config-noise");
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let noise: Vec<u8> = (0..1_000_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    assert!(std::str::from_utf8(&noise).is_err());
    let config = dir.join("noise.toml");
    std::fs::write(&config, &noise).unwrap();
    // The binary is built before the time starts.
    let mut tiza = tiza_qa::command();
    tiza.arg("config").env("TIZA_CONFIG", &config);
    let t = Instant::now();
    let out = tiza.output().unwrap();
    let took = t.elapsed();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // About 0.25 s in release and 3 s in debug here; the shared CI runners
    // are two to three times slower, so the debug budget is wide (the
    // release budget is the one that counts).
    let budget = if cfg!(debug_assertions) { 20.0 } else { 1.0 };
    assert!(took.as_secs_f64() < budget, "{took:?}");
}
