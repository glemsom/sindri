//! Copilot setup is explicit: configuration must name device sign-in before a run.

#![allow(
    clippy::expect_used,
    reason = "a panicking test reports a broken contract"
)]

use sindri_core::config::{Config, Overrides};
use tempfile::TempDir;

#[test]
fn copilot_without_saved_login_points_to_the_device_sign_in_command() {
    let home = TempDir::new().expect("home");
    let config = home.path().join("config");
    let state = home.path().join("state");
    std::fs::create_dir_all(config.join("sindri")).expect("config directory");
    std::fs::write(
        config.join("sindri/config.toml"),
        r#"provider = "copilot"
model = "gpt-4o"
"#,
    )
    .expect("config file");
    // SAFETY: this test is run alone; it uses private config/state roots and
    // clears provider overrides before resolving the configuration.
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", &config);
        std::env::set_var("XDG_STATE_HOME", &state);
        std::env::remove_var("SINDRI_PROVIDER");
        std::env::remove_var("SINDRI_MODEL");
        std::env::remove_var("SINDRI_API_KEY");
    }
    let error = Config::resolve(Overrides::default()).expect_err("no Copilot login");
    assert!(error.message().contains("sindri auth login copilot"));
}
