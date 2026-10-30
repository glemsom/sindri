//! The process seam for batch mode: a prompt in, the answer out, and every
//! refusal of the front door. No network: the provider is a mock server on
//! loopback that the run is pointed at.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::Env;
use common::provider::{MockProvider, Reply};
use common::{batch, stderr, stdout};

/// Runs the binary with a stdin pipe the parent holds open and never writes
/// to. A front end that drains stdin would block here forever; one that
/// refuses a pipe without draining it exits at once.
fn run_with_open_stdin(env: &Env, args: &[&str]) -> (i32, String, String) {
    let mut child: Child = Command::new(assert_cmd::cargo::cargo_bin!("sindri"))
        .args(args)
        .current_dir(&env.workspace)
        .env("XDG_STATE_HOME", &env.state)
        .env("XDG_CONFIG_HOME", &env.config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sindri");

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("sindri blocked on an open stdin pipe instead of refusing it");
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    child
        .stdout
        .take()
        .expect("stdout")
        .read_to_string(&mut stdout)
        .expect("read stdout");
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut stderr)
        .expect("read stderr");
    (status.code().expect("exit code"), stdout, stderr)
}

#[test]
fn piped_stdin_without_batch_is_refused_and_never_drained() {
    let env = Env::new();
    let (code, stdout, stderr) = run_with_open_stdin(&env, &[]);

    assert_eq!(code, 2, "a piped stdin without -b is a usage error");
    assert_eq!(stdout, "", "a refusal writes nothing to stdout");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "one stderr line, got {stderr:?}");
    assert!(lines[0].contains("-b"), "the refusal names -b: {stderr:?}");
}

const TEST_KEY: &str = common::TEST_KEY;

#[test]
fn the_answer_and_one_newline_are_the_only_things_on_stdout() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Two files changed: one test, one doc.")]);
    let output = batch(&env, &mock, &["-b", "summarise this"], "--- a/x\n+++ b/x\n");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "Two files changed: one test, one doc.\n",
        "stdout is the answer plus exactly one newline"
    );
    assert_eq!(stderr(&output), "", "stderr stays silent on success");

    let requests = mock.requests();
    assert_eq!(requests.len(), 1, "one turn, one request");
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/messages");
    assert_eq!(
        request.header("x-api-key"),
        Some(TEST_KEY),
        "the credential from the environment is sent"
    );

    let body = request.body.clone();
    assert!(
        body.contains("claude-sonnet-4-6"),
        "the configured model is asked for: {body}"
    );
    let prompt = body.find("summarise this").expect("the prompt is sent");
    let context = body.find("+++ b/x").expect("the piped stdin is sent");
    assert!(
        prompt < context,
        "stdin is context after the prompt: {body}"
    );
}

#[test]
fn batch_with_no_prompt_argument_takes_stdin_as_the_prompt() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("A rename in two files.")]);
    let output = batch(&env, &mock, &["-b"], "what did I just change?\n");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "A rename in two files.\n");

    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].body.contains("what did I just change?"),
        "stdin itself is the prompt: {}",
        requests[0].body
    );
}

#[test]
fn a_batch_run_with_no_prompt_at_all_is_a_usage_error() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("never sent")]);
    let output = batch(&env, &mock, &["-b"], "   \n");

    assert_eq!(output.status.code(), Some(2), "no prompt is a usage error");
    assert_eq!(stdout(&output), "");
    assert_eq!(stderr(&output).lines().count(), 1, "one stderr line");
    assert!(
        stderr(&output).contains("prompt"),
        "the refusal says what is missing: {}",
        stderr(&output)
    );
    assert_eq!(mock.requests().len(), 0, "no prompt sends no request");
}

#[test]
fn a_trailing_newline_from_the_model_does_not_become_two() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("Two files changed.\n\n")]);
    let output = batch(&env, &mock, &["-b", "summarise this"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "Two files changed.\n",
        "the answer is followed by exactly one newline, whatever the model sent"
    );
}

#[test]
fn batch_mode_alongside_a_subcommand_is_a_usage_error() {
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("never sent")]);
    let output = batch(&env, &mock, &["-b", "summarise this", "doctor"], "");

    assert_eq!(
        output.status.code(),
        Some(2),
        "one run, one front end: {}",
        stderr(&output)
    );
    assert_eq!(stdout(&output), "");
    assert_eq!(mock.requests().len(), 0, "nothing is asked");
}
