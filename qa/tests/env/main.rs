//! Tests that change the environment of their own process. They are alone
//! in their crate, and run one at a time: each holds [`ENV_LOCK`] while it
//! changes or reads the environment.

mod developer_config;
mod socket_dir_env;

/// Held by each test of this crate for all of its run.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
