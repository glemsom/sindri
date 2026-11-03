//! The reports a batch run sends the terminal it draws in: `OSC 7501` on stderr,
//! in order, and never a byte of the answer's stream. Driven through a pty,
//! because "is stderr a terminal" is exactly the question the channel answers —
//! with stdout in a pipe beside it, because the answer is a contract a script
//! reads whole and a report must never reach it.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use std::io::Read;
use std::process::Child;
use std::time::{Duration, Instant};

use common::provider::{MockProvider, Reply};
use common::pty::{DEADLINE, Pty, spawn_with_answer_piped};
use common::{Env, TEST_KEY, signal, stderr, stdout, wait_until, write_config};

/// One batch run on a terminal, read to the end: what it told the pty it was
/// doing, in arrival order, the answer it printed, and the code it left by.
struct Ran {
    reports: Vec<String>,
    answer: String,
    code: Option<i32>,
}

impl Ran {
    /// The `state=` of each report, in order.
    fn states(&self) -> Vec<&str> {
        self.reports
            .iter()
            .map(|body| field(body, "state="))
            .collect()
    }

    /// Every message the run said, base64 undone: the words a test reads.
    fn messages(&self) -> Vec<String> {
        self.reports.iter().map(|body| decoded(body)).collect()
    }

    /// The one report that settled the run.
    fn settled(&self) -> &str {
        self.reports.last().expect("a run says how it ended")
    }
}

/// A batch run on a pty whose stdout is a pipe, as it runs: the two tests that
/// end a run themselves — a signal, a deadline — need the child in reach.
struct Running {
    pty: Pty,
    child: Child,
    drawn: String,
}

impl Running {
    /// Batched through `-b`, so a test names only what the run is asked and how
    /// its terminal is wired. `None` where the machine has no pty to offer: a
    /// container without `/dev/ptmx` cannot test a channel gated on one, and
    /// skipping says so rather than passing quietly.
    fn start(
        env: &Env,
        mock: &MockProvider,
        args: &[&str],
        vars: &[(&str, &str)],
    ) -> Option<Running> {
        let pty = Pty::new()?;
        let base = mock.base_url();
        let mut child_vars: Vec<(&str, &str)> = vec![
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_MODEL", "claude-sonnet-4-6"),
            ("SINDRI_BASE_URL", base.as_str()),
        ];
        child_vars.extend(vars.iter().copied());
        let child = spawn_with_answer_piped(env, &pty, args, &child_vars);
        Some(Running {
            pty,
            child,
            drawn: String::new(),
        })
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The pty read to the end of the run: the reports arrive while it works, so
    /// the terminal is drained as it goes rather than once it has exited.
    fn finish(mut self) -> Ran {
        let deadline = Instant::now() + DEADLINE;
        let mut code = None;
        while Instant::now() < deadline {
            self.pty.drain(&mut self.drawn);
            if let Ok(Some(status)) = self.child.try_wait() {
                code = status.code();
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // Whatever was said between the last drain and the exit.
        self.pty.drain(&mut self.drawn);

        let mut answer = String::new();
        if let Some(mut out) = self.child.stdout.take() {
            out.read_to_string(&mut answer).expect("the answer is read");
        }
        Ran {
            reports: self
                .pty
                .status()
                .iter()
                .map(|status| status.body().to_string())
                .collect(),
            answer,
            code,
        }
    }
}

/// One named field of a report's body, or a panic naming the report that has no
/// such field: a report is key:value pairs and nothing a terminal has to imply.
fn field<'a>(body: &'a str, name: &str) -> &'a str {
    body.split(':')
        .find_map(|field| field.strip_prefix(name))
        .unwrap_or_else(|| panic!("a report carries `{name}`: {body}"))
}

/// A report's `msg`, base64 undone: the one field a test reads for its words.
fn decoded(body: &str) -> String {
    use base64::Engine as _;
    let payload = field(body, "msg=");
    String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("a report's message is base64"),
    )
    .expect("a message is utf8")
}

fn answering() -> MockProvider {
    MockProvider::start(vec![Reply::answer("The tree holds one crate.")])
}

#[test]
fn a_batch_run_tells_the_terminal_it_is_working_and_then_done() {
    let env = Env::new();
    let mock = answering();
    let Some(running) = Running::start(&env, &mock, &["-b", "what is in the tree"], &[]) else {
        eprintln!("no pty on this machine: batch mode's reports are not tested here");
        return;
    };

    let ran = running.finish();

    // One report before the run, one when it settles, and whatever change of
    // what it was doing came between them.
    let states = ran.states();
    assert_eq!(
        (states.first(), states.last()),
        (Some(&"working"), Some(&"done")),
        "a run says it is working and then that it is done, in that order: {:?}",
        ran.reports
    );
    assert!(
        decoded(&ran.reports[0]).starts_with("turn 1 · thinking"),
        "the first report says the first turn is being thought about: {:?}",
        ran.messages()
    );
    for body in &ran.reports {
        assert_eq!(
            field(body, "app="),
            "sindri",
            "every report names the application a terminal keys the record by: {body}"
        );
        assert!(
            body.len() < 4096,
            "a report past the ceiling a terminal discards whole is never sent: {body}"
        );
    }
    assert_eq!(
        ran.answer, "The tree holds one crate.\n",
        "stdout is the answer"
    );
    assert_eq!(ran.code, Some(0));
}

#[test]
fn a_run_that_spent_its_turn_cap_says_done_and_names_the_ceiling() {
    let env = Env::new();
    // A model that asks for a command on every turn, so the cap is what ends
    // the run and no answer is ever written.
    let mock = MockProvider::start(vec![Reply::tool_call("toolu_ls", "echo hello", "Looking.")]);
    let started = Running::start(
        &env,
        &mock,
        &["-b", "keep looking"],
        &[("SINDRI_MAX_TURNS", "2")],
    );
    let Some(running) = started else {
        eprintln!("no pty on this machine: batch mode's reports are not tested here");
        return;
    };

    let ran = running.finish();

    assert_eq!(ran.code, Some(100), "the cap is said in the exit code too");
    assert_eq!(
        field(ran.settled(), "state="),
        "done",
        "a cap is a run that was told to stop and did: {:?}",
        ran.reports
    );
    assert!(
        decoded(ran.settled()).contains("turn cap"),
        "the message names the ceiling it hit: {:?}",
        ran.messages()
    );
}

#[test]
fn a_run_that_could_not_deliver_says_error() {
    let env = Env::new();
    // A request the provider refuses: the loop never reaches a stop reason, so
    // the only word left for the run is that it could not deliver.
    let mock = MockProvider::start(vec![Reply::error(
        400,
        "invalid_request_error",
        "model: field required",
    )]);
    let Some(running) = Running::start(&env, &mock, &["-b", "what is in the tree"], &[]) else {
        eprintln!("no pty on this machine: batch mode's reports are not tested here");
        return;
    };

    let ran = running.finish();

    assert_eq!(
        field(ran.settled(), "state="),
        "error",
        "a run that could not deliver says so: {:?}",
        ran.reports
    );
    assert_eq!(ran.answer, "", "and writes nothing to stdout");
    assert_eq!(ran.code, Some(1));
}

#[test]
fn sigint_says_idle_on_stderr_and_leaves_stdout_empty() {
    let env = Env::new();
    let started = env.workspace.join("started");
    let script = format!("touch {}; sleep 30", started.display());
    let mock = MockProvider::start(vec![Reply::tool_call("toolu_slow", &script, "Working.")]);
    let Some(running) = Running::start(&env, &mock, &["-b", "probe the tree"], &[]) else {
        eprintln!("no pty on this machine: batch mode's reports are not tested here");
        return;
    };

    wait_until(&started);
    signal(running.pid(), libc::SIGINT);
    let ran = running.finish();

    assert_eq!(
        field(ran.settled(), "state="),
        "idle",
        "an interrupted program leaves the terminal as it found it: {:?}",
        ran.reports
    );
    assert_eq!(ran.answer, "", "a cancelled run writes nothing to stdout");
    assert_eq!(ran.code, Some(130));
}

#[test]
fn a_stderr_that_is_a_file_holds_no_escape_and_the_answer_is_the_same_bytes() {
    let env = Env::new();
    let mock = answering();
    let log = env.workspace.join("stderr.log");

    let redirected = env
        .command()
        .args(["-b", "what is in the tree"])
        .env("SINDRI_API_KEY", TEST_KEY)
        .env("SINDRI_MODEL", "claude-sonnet-4-6")
        .env("SINDRI_BASE_URL", mock.base_url())
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(
            std::fs::File::create(&log).expect("a log beside the run"),
        ))
        .output()
        .expect("the run answers");

    assert_eq!(
        stdout(&redirected),
        "The tree holds one crate.\n",
        "a report must never reach the answer's stream"
    );
    assert_eq!(stderr(&redirected), "", "the file is not a terminal");
    let written = std::fs::read_to_string(&log).expect("the log is read");
    assert!(
        !written.contains('\u{1b}'),
        "an escape handed to a file is bytes in the file: {written:?}"
    );
}

#[test]
fn the_setting_silences_every_report_and_leaves_the_answer_alone() {
    let env = Env::new();
    write_config(&env, "status = false\n");
    let mock = answering();
    let Some(running) = Running::start(&env, &mock, &["-b", "what is in the tree"], &[]) else {
        eprintln!("no pty on this machine: batch mode's reports are not tested here");
        return;
    };

    let ran = running.finish();

    assert_eq!(ran.reports, Vec::<String>::new(), "the switch said nothing");
    assert_eq!(
        ran.answer, "The tree holds one crate.\n",
        "and the answer is untouched"
    );
}

#[test]
fn no_report_carries_the_prompt_the_command_or_the_answer() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::tool_call("toolu_ls", "echo sk-live-9f2c", "Looking."),
        Reply::answer("The tree holds one crate."),
    ]);
    let prompt = "list the tree and never say sk-live-9f2c";
    let Some(running) = Running::start(&env, &mock, &["-b", prompt], &[]) else {
        eprintln!("no pty on this machine: batch mode's reports are not tested here");
        return;
    };

    let ran = running.finish();

    assert!(ran.reports.len() >= 3, "the run reported as it went");
    for body in &ran.reports {
        let said = decoded(body);
        for secret in [prompt, "sk-live-9f2c", "echo", "The tree holds one crate"] {
            assert!(
                !body.contains(secret) && !said.contains(secret),
                "a message is our own coarse words and never {secret:?}: {body} / {said}"
            );
        }
    }
}

#[test]
fn a_json_run_prints_one_object_on_stdout_and_reports_beside_it() {
    let env = Env::new();
    let mock = answering();
    let Some(running) = Running::start(
        &env,
        &mock,
        &["-b", "what is in the tree", "--format", "json"],
        &[],
    ) else {
        eprintln!("no pty on this machine: batch mode's reports are not tested here");
        return;
    };

    let ran = running.finish();

    assert_eq!(
        ran.answer.matches('\n').count(),
        1,
        "--format json is one line, whatever the terminal is told: {:?}",
        ran.answer
    );
    let object: serde_json::Value =
        serde_json::from_str(ran.answer.trim_end()).expect("one object");
    assert_eq!(object["answer"], "The tree holds one crate.");
    assert_eq!(object["stopped"], "final");
    let states = ran.states();
    assert_eq!(
        (states.first(), states.last()),
        (Some(&"working"), Some(&"done")),
        "reports go to stderr only, and they go there: {:?}",
        ran.reports
    );
}
