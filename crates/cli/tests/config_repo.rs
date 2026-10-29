//! A config committed in a repository is never honoured, and the refusal is
//! said once on stderr (ADR-0005): cloning an untrusted repo must not grant the
//! agent extra writable paths.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use common::{Env, TEST_KEY, stderr, stdout};

/// A directory under the workspace that the repository's config asks for.
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
