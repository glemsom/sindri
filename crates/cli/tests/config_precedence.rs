//! Precedence across the whole schema: file, then environment, then flags
//! (ADR-0005). Each setting is proven by what the run observably does — which
//! endpoint it calls, how many turns it takes, how long a command may run.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use common::{Env, stderr, stdout, write_config};

/// The run, through the shared harness: exactly the settings the test supplies,
/// so a test owns every level of the precedence.
fn run(env: &Env, args: &[&str], vars: &[(&str, &str)]) -> std::process::Output {
    common::run_with(env, args, vars)
}

#[test]
fn the_base_url_comes_from_the_file_then_the_environment_then_a_flag() {
    let env = Env::new();
    let from_file = MockProvider::start(vec![Reply::answer("from file")]);
    let from_env = MockProvider::start(vec![Reply::answer("from env")]);
    let from_flag = MockProvider::start(vec![Reply::answer("from flag")]);
    write_config(
        &env,
        &format!(
            "model = \"claude-from-file\"\nbase_url = \"{}\"\n",
            from_file.base_url()
        ),
    );

    let output = run(
        &env,
        &["-b", "q"],
        &[("SINDRI_BASE_URL", &from_env.base_url())],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "from env\n", "the environment wins");
    assert_eq!(from_file.requests().len(), 0, "the file's URL is unused");
    let environment_served = from_env.requests().len();

    let output = run(
        &env,
        &["-b", "q", "--base-url", &from_flag.base_url()],
        &[("SINDRI_BASE_URL", &from_env.base_url())],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "from flag\n", "the flag wins");
    assert_eq!(
        from_env.requests().len(),
        environment_served,
        "the environment's URL is unused once a flag names one"
    );
}

#[test]
fn the_turn_cap_comes_from_the_file_then_the_environment_then_a_flag() {
    let env = Env::new();
    write_config(&env, "model = \"claude-from-file\"\nmax_turns = 1\n");
    let looping = || {
        MockProvider::start(vec![
            Reply::tool_call("t1", "true", ""),
            Reply::tool_call("t2", "true", ""),
            Reply::tool_call("t3", "true", ""),
        ])
    };

    let mock = looping();
    let output = run(&env, &["-b", "q"], &[("SINDRI_BASE_URL", &mock.base_url())]);
    assert_eq!(
        output.status.code(),
        Some(100),
        "the file's cap stops the run"
    );
    assert_eq!(mock.requests().len(), 1, "a cap of one takes one turn");

    let mock = looping();
    let output = run(
        &env,
        &["-b", "q"],
        &[
            ("SINDRI_BASE_URL", &mock.base_url()),
            ("SINDRI_MAX_TURNS", "2"),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(100),
        "the environment's cap stops it"
    );
    assert_eq!(
        mock.requests().len(),
        2,
        "the environment overrides the file"
    );

    let mock = looping();
    let output = run(
        &env,
        &["-b", "q", "--max-turns", "3"],
        &[
            ("SINDRI_BASE_URL", &mock.base_url()),
            ("SINDRI_MAX_TURNS", "2"),
        ],
    );
    assert_eq!(output.status.code(), Some(100), "the flag's cap stops it");
    assert_eq!(
        mock.requests().len(),
        3,
        "the flag overrides the environment"
    );
}

#[test]
fn the_command_timeout_comes_from_the_file_then_the_environment_then_a_flag() {
    let env = Env::new();
    write_config(&env, "model = \"claude-from-file\"\ntimeout = 1\n");

    // The file's one-second deadline really is enforced.
    let mock = MockProvider::start(vec![
        Reply::tool_call("t1", "sleep 30", ""),
        Reply::answer("gave up"),
    ]);
    let started = std::time::Instant::now();
    let output = run(&env, &["-b", "q"], &[("SINDRI_BASE_URL", &mock.base_url())]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "the file's deadline bounded the command, took {elapsed:?}"
    );
    assert!(
        mock.requests()[1].body.contains("outcome: timed_out"),
        "the command timed out under the file's deadline"
    );

    // A flag raises it above the file's, so the same command finishes.
    let mock = MockProvider::start(vec![
        Reply::tool_call("t1", "sleep 2; echo done", ""),
        Reply::answer("finished"),
    ]);
    let output = run(
        &env,
        &["-b", "q", "--timeout", "30"],
        &[("SINDRI_BASE_URL", &mock.base_url())],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "finished\n");
    assert!(
        mock.requests()[1].body.contains("outcome: exited"),
        "the flag's deadline overrode the file's, so the command exited"
    );
}

#[test]
fn the_provider_comes_from_the_file_and_a_flag_overrides_it() {
    let env = Env::new();
    // A provider no wire claims: the file's value is read, and refused.
    write_config(
        &env,
        "model = \"claude-from-file\"\nprovider = \"gemini\"\n",
    );
    let mock = MockProvider::start(vec![Reply::answer("ok")]);

    let output = run(
        &env,
        &["-b", "q"],
        &[
            ("SINDRI_BASE_URL", &mock.base_url()),
            ("SINDRI_MODEL", "claude-sonnet-4-6"),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "the file's provider is the one refused: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("gemini"),
        "the error names the provider: {}",
        stderr(&output)
    );

    let output = run(
        &env,
        &["-b", "q", "--provider", "anthropic"],
        &[("SINDRI_BASE_URL", &mock.base_url())],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        mock.requests().len(),
        1,
        "the flag's provider served the run"
    );
}
