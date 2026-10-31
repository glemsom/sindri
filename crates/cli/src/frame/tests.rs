//! The frame's own seams, without a terminal: the rail facts a launch hands the
//! TUI, and the posture the sandbox policy gives them.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken seam is reported"
)]

use std::time::Duration;

use sindri_core::config::Provider;
use sindri_core::session::Env as RunEnv;
use tempfile::TempDir;

use super::*;

fn config() -> Config {
    Config {
        provider: Provider::Anthropic,
        model: "model-one".to_string(),
        api_key: String::new(),
        base_url: None,
        max_turns: 3,
        timeout: Duration::from_secs(5),
        thinking: false,
        extra_writable: Vec::new(),
        enforcement: Enforcement::Bubblewrap,
        extra_headers: Vec::new(),
        refused_repo_config: None,
    }
}

#[test]
fn the_rail_states_your_own_privileges_when_the_run_left_the_sandbox() {
    let root = TempDir::new().expect("temp root");
    let policy = sindri_core::session::policy_for(
        &root.path().canonicalize().expect("canonical"),
        &[],
        root.path().join("tmp"),
        "/usr/bin:/bin".to_string(),
        Enforcement::YoloUnsafe,
    )
    .expect("policy");

    assert_eq!(
        posture(&policy),
        "unsafe · your own privileges",
        "the chip names the posture the run really has"
    );
}

/// A run environment in a temporary directory, so nothing the launch says
/// is the process's own.
fn env(root: &TempDir) -> RunEnv {
    let workspace = root.path().join("workspace");
    let state = root.path().join("state");
    for dir in [&workspace, &state] {
        std::fs::create_dir_all(dir).expect("a temporary directory");
    }
    RunEnv {
        workspace: workspace.canonicalize().expect("canonical workspace"),
        search_path: "/usr/bin:/bin".to_string(),
        state,
    }
}

#[test]
fn the_rail_names_the_workspace_the_sandbox_actually_mounts() {
    let root = TempDir::new().expect("temp root");
    let boot = boot::Configured {
        env: env(&root),
        config: config(),
    }
    .start()
    .expect("a session in the temporary environment");
    // Asked of the policy, not of the process.
    let mounted = boot
        .session
        .policy()
        .writable()
        .paths()
        .iter()
        .find(|path| path.ends_with("workspace"))
        .expect("the workspace is writable inside the sandbox")
        .clone();

    let launch = launch(&boot);

    // Shortened only by the home it is under: the mounted directory, not
    // the process's own.
    let shown = launch.workspace.trim_start_matches("~/");
    assert!(
        mounted.display().to_string().ends_with(shown),
        "the rail states the workspace the sandbox mounts: {shown:?} of {mounted:?}"
    );
    assert_eq!(
        launch.temp,
        boot.session.dir().temp().display().to_string(),
        "and the temp the session maps, beside it"
    );
    assert_eq!(launch.model, "model-one", "the model the run asks");
}
