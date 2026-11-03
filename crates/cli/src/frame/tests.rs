//! The frame's own seams, without a terminal: the rail facts a launch hands the
//! TUI, and the posture the sandbox policy gives them.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken seam is reported"
)]

use std::time::Duration;

use sindri_core::config::Provider;
use sindri_core::sandbox::Extras;
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
        status: true,
        extras: Extras::Named(Vec::new()),
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
        &sindri_core::sandbox::Extras::Named(Vec::new()),
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

/// The record's failure is one sentence, and the frame draws the one the run
/// writes: the record's own directory, named the way batch mode names it,
/// rather than a second spelling with a session id in its place.
#[test]
fn a_record_that_cannot_be_finished_is_drawn_in_the_sentence_the_run_writes() {
    let root = TempDir::new().expect("temp root");
    let boot = boot::Configured {
        env: env(&root),
        config: config(),
    }
    .start()
    .expect("a session in the temporary environment");
    std::fs::remove_dir_all(boot.session.dir().root()).expect("remove the record's directory");

    let note = closing_note(
        &boot.session,
        &Ok(Report {
            answer: "Twelve files.".to_string(),
            turns: 1,
            stopped: sindri_core::agent::Stopped::Final,
            model: None,
            tool_calls: 0,
            spent: Default::default(),
            usage: Default::default(),
            duration: Duration::from_millis(1),
        }),
    )
    .expect("a record that cannot be finished has a note to say so");

    assert!(
        note.starts_with(&format!(
            "session {}: the record could not be finished: ",
            boot.session.dir().root().display()
        )),
        "the note is the run's one sentence, naming the record's directory: {note}"
    );
}

/// One probe of one repository: the branch the rail states is the branch the
/// record holds, because the frame reads the session's own value and never
/// shells git a second time.
#[test]
fn the_rail_and_the_record_share_one_git_lookup() {
    let root = TempDir::new().expect("temp root");
    let mut env = env(&root);
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=sindri",
                "-c",
                "user.email=sindri@example.invalid",
            ])
            .args(args)
            .current_dir(&env.workspace)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q", "--initial-branch=the-branch"]);
    std::fs::write(env.workspace.join("readme"), "one line\n").expect("a file to commit");
    git(&["add", "readme"]);
    git(&["commit", "-q", "-m", "the first commit"]);
    env.workspace = env.workspace.canonicalize().expect("canonical workspace");

    let boot = boot::Configured {
        env,
        config: config(),
    }
    .start()
    .expect("a session in the repository");

    let launch = launch(&boot);
    assert_eq!(
        launch.branch.as_deref(),
        Some("the-branch"),
        "the rail states the branch the record probed"
    );
    let stored = sindri_core::record::Stored::load_in(boot.session.dir()).expect("the record");
    assert_eq!(
        launch.branch.as_deref(),
        stored.meta.git.as_ref().map(|git| git.branch.as_str()),
        "one lookup, so the two cannot disagree"
    );
}
