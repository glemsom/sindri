//! A config committed in a repository is never honoured, and the refusal is
//! said once on stderr wherever a config is resolved: cloning an
//! untrusted repo must not grant the agent extra writable paths, and no front
//! end may hide the refusal — batch mode, the frame, `sandbox exec` and `doctor`
//! all say it, and a settings save inside the frame says nothing at all.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use std::process::Child;

use common::pty::{
    ALTERNATE_SCREEN, CLEAR_FIELD, DOWN, ENTER, Pty, expect, spawn, spawn_with_stderr, type_command,
};
use common::{Env, stderr, stdout};

/// How many rows down the settings modal's `extra_writable_paths` field sits
/// from where it opens: the schema's order, which the modal draws.
const SETTINGS_EXTRAS_ROW: usize = 8;

fn extra(env: &Env) -> std::path::PathBuf {
    let dir = env.workspace.parent().expect("root").join("repo-grant");
    std::fs::create_dir_all(&dir).expect("extra dir");
    dir.canonicalize().expect("canonical extra")
}

#[test]
fn a_repository_config_is_ignored_and_the_refusal_is_said_once() {
    let env = Env::new();
    let extra = extra(&env);
    std::fs::write(
        env.workspace.join(".sindri.toml"),
        format!(
            "model = \"claude-from-the-repository\"\nextra_writable_paths = [\"{}\"]\n",
            extra.display()
        ),
    )
    .expect("write repository config");

    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = common::batch(&env, &mock, &["-b", "summarise this"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "ok\n", "stdout stays the answer only");

    let requests = mock.requests();
    assert_eq!(requests.len(), 1, "one run makes one request");
    let body = requests[0].body.clone();
    assert!(
        !body.contains("claude-from-the-repository"),
        "the repository's model is not used: {body}"
    );
    assert!(
        !body.contains(&extra.display().to_string()),
        "the repository's writable grant is not mounted or described: {body}"
    );

    let message = stderr(&output);
    assert_eq!(
        message.matches(".sindri.toml").count(),
        1,
        "the refusal is noted exactly once: {message}"
    );
    assert!(
        message.contains("ignor"),
        "the note says the file was ignored: {message}"
    );
}

#[test]
fn a_run_without_a_repository_config_says_nothing_about_one() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let output = common::batch(&env, &mock, &["-b", "summarise this"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        stderr(&output),
        "",
        "a silent run stays silent when there is nothing to refuse"
    );
}

fn config_for(env: &Env, mock: &MockProvider) {
    common::write_config(
        env,
        &format!(
            "provider = \"anthropic\"\nmodel = \"model-one\"\nbase_url = \"{}\"\n",
            mock.base_url()
        ),
    );
}

fn committed_config(env: &Env) {
    std::fs::write(
        env.workspace.join(".sindri.toml"),
        "model = \"claude-from-the-repository\"\nextra_writable_paths = [\"/tmp/repo-grant\"]\n",
    )
    .expect("write repository config");
}

fn launch(env: &Env, pty: &Pty, mock: &MockProvider, told: Option<&std::path::Path>) -> Child {
    config_for(env, mock);
    committed_config(env);
    let base_url = mock.base_url();
    let vars = [
        ("SINDRI_API_KEY", common::TEST_KEY),
        ("SINDRI_BASE_URL", base_url.as_str()),
    ];
    match told {
        Some(file) => spawn_with_stderr(env, pty, &vars, file),
        None => spawn(env, pty, &vars),
    }
}

#[test]
fn sandbox_exec_announces_the_refused_repository_config_once() {
    let env = Env::new();
    committed_config(&env);

    let output = env
        .cmd()
        .args(["sandbox", "exec", "--", "true"])
        .assert()
        .get_output()
        .clone();

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let outcome: serde_json::Value =
        serde_json::from_str(&stdout(&output)).expect("stdout stays the outcome only");
    assert_eq!(outcome["outcome"], "exited", "{}", stdout(&output));
    let message = stderr(&output);
    assert_eq!(
        message.matches(".sindri.toml").count(),
        1,
        "the diagnostic says the refusal exactly once: {message}"
    );
    assert!(message.contains("ignor"), "and says what it did: {message}");
}

#[test]
fn a_repository_config_is_announced_once_before_the_frame_draws_anything() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the frame's note is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let mut child = launch(&env, &pty, &mock, None);

    let drawn = pty.wait_for(&mut child, ALTERNATE_SCREEN);
    let said = drawn.matches(".sindri.toml").count();
    assert_eq!(
        said, 1,
        "the launch says the refusal exactly once: {drawn:?}"
    );
    let note = drawn.find(".sindri.toml").expect("the note");
    let first_frame = drawn.find(ALTERNATE_SCREEN).expect("the first frame");
    assert!(
        note < first_frame,
        "and says it before the first frame is drawn: {drawn:?}"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
}

#[test]
fn a_settings_save_inside_the_frame_says_nothing_on_stderr() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the save's silence is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("ok")]);
    let scratch = env
        .workspace
        .parent()
        .expect("a workspace's parent")
        .join("scratch");
    std::fs::create_dir_all(&scratch).expect("scratch");
    let scratch = scratch.canonicalize().expect("canonical scratch");
    let told = env.state.join("stderr");
    let mut child = launch(&env, &pty, &mock, Some(&told));

    let opened = pty.wait_for_screen(&mut child, "bwrap · 1 writable");
    assert!(!opened.is_empty(), "the frame is drawing");
    let launch = std::fs::read_to_string(&told).expect("the launch's stderr");
    assert_eq!(
        launch.matches(".sindri.toml").count(),
        1,
        "the launch said the refusal once: {launch:?}"
    );

    // A save happens while the frame owns the screen: it takes the failure as a
    // value, and a line on stderr there would corrupt the frame.
    type_command(&pty, "/settings");
    expect(&pty, &mut child, "extra_writable_paths");
    let mut port = pty.port();
    for _ in 0..SETTINGS_EXTRAS_ROW {
        std::io::Write::write_all(&mut port, DOWN).expect("a cursor move");
    }
    drop(port);
    pty.type_key(CLEAR_FIELD);
    for byte in scratch.display().to_string().bytes() {
        pty.type_key(byte);
    }
    pty.type_key(ENTER);
    expect(&pty, &mut child, "settings saved");
    // The reload is what would be tempted to say the refusal again, so the
    // posture is waited for: the note must not arrive with it.
    pty.wait_for_screen(&mut child, "bwrap · 2 writable");

    let saved = std::fs::read_to_string(&told).expect("stderr after the save");
    assert_eq!(
        saved, launch,
        "saving settings inside the frame wrote nothing to stderr"
    );

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert_eq!(
        std::fs::read_to_string(&told).expect("stderr at the end"),
        launch,
        "and nothing on the way out either"
    );
}
