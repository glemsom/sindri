//! What a batch run reports to whoever scripted it: §15's object, one field after
//! another in a fixed order, a field the run knows nothing about left out rather
//! than written as `null`, and the exit code read off the stop reason and from
//! nowhere else.
//!
//! Every run here is a scripted model and a scripted tool, so no test reaches a
//! network or spends a second: what a call cost is a number a test states, and
//! the report's `command_ms` and `output_bytes` are those numbers summed.

use std::time::Duration;

use sindri_core::outcome::{Facts, Outcome};
use sindri_core::policy::Enforcement;
use sindri_core::provider::{Deltas, Failure};
use sindri_core::records::Usage;
use sindri_core::redact::{Redacted, SecretSet};
use sindri_core::run::{
    Acted, Call, Executor, Exit, Limits, Loaded, Reply, Request, Responder, Run, State, Stop,
};

/// A model that says one thing and asks for nothing.
struct Says(&'static str);

impl Responder for Says {
    fn respond(&self, _request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        deltas.text(self.0);
        Ok(Reply::answer(self.0))
    }
}

/// A model whose every request fails, in the one line a failed run prints.
struct Fails;

impl Responder for Fails {
    fn respond(&self, _request: &Request, _deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        Err(Failure {
            verdict: sindri_core::provider::Verdict::Client,
            retryable: false,
            streamed: false,
            message: SecretSet::default().redact("the wire refused the request"),
        })
    }
}

/// A model that asks for one more command every turn, so no turn is final.
struct Loopy;

impl Responder for Loopy {
    fn respond(&self, _request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        deltas.text("more");
        Ok(Reply::asking(
            "more",
            [Call::new("bash", r#"{"command":"true"}"#)],
        ))
    }
}

/// A tool that costs the run one wall clock and one output size, whatever it is
/// asked, and refuses nothing.
struct Costs;

impl Executor for Costs {
    fn execute(&self, _call: &Call, _loaded: &mut Loaded) -> Result<Acted, Redacted> {
        Ok(Acted {
            text: String::from("exit 0\n"),
            bytes: 64,
            elapsed: Duration::from_millis(5),
            exit: Exit::Code(0),
        })
    }
}

/// The sink a run's fragments land in when nothing is watching them.
struct Quiet;

impl Deltas for Quiet {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

/// The facts a report is given that the run itself does not hold.
fn facts<'a>(session: &'a str, model: &'a str) -> Facts<'a> {
    Facts {
        session,
        model,
        enforcement: Enforcement::Bubblewrap,
        duration: Duration::from_millis(1234),
        usage: Usage::default(),
    }
}

/// The report of `run`, driven to whatever stop reason it reaches.
fn reported<R: Responder, A: Executor>(prompt: &str, responder: R, executor: A) -> Outcome {
    let mut run = Run::new(prompt, responder, executor);
    let mut quiet = Quiet;
    let state = run.drive(&mut quiet);
    assert!(
        !matches!(state, State::Running),
        "a driven run stops, which is what makes a report possible"
    );
    Outcome::of(
        &run,
        facts("019fd6ff-2b7a-7c31-9d6c-0b6b0e0f4a11", "claude-sonnet-4-6"),
    )
    .expect("a stopped run has an outcome")
}

/// The field names of `json` at the object's own depth, in the order the bytes
/// write them: `serde_json::Value` sorts an object's keys, so the order is read
/// off the line itself rather than through a map that has forgotten it. A string
/// is a key when a colon follows it and a value otherwise.
fn keys(json: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut pending: Option<String> = None;
    let mut name = String::new();
    let mut opened_at = 0usize;
    for character in json.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
                pending = (opened_at == 1).then(|| name.clone());
            } else if opened_at == 1 {
                name.push(character);
            }
            continue;
        }
        if let Some(settled) = pending.take()
            && character == ':'
        {
            keys.push(settled);
        }
        match character {
            '"' => {
                in_string = true;
                name.clear();
                opened_at = depth;
            }
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            _ => {}
        }
    }
    keys
}

#[test]
fn the_report_names_its_fields_in_the_order_a_script_reads_them() {
    let mut run = Run::new("hello", Says("hi"), Costs);
    let mut quiet = Quiet;
    run.drive(&mut quiet);
    let mut told = facts("019fd6ff-2b7a-7c31-9d6c-0b6b0e0f4a11", "claude-sonnet-4-6");
    told.usage = Usage {
        input: Some(11),
        ..Usage::default()
    };
    let json = Outcome::of(&run, told).expect("a stopped run").json();

    assert_eq!(
        keys(&json),
        vec![
            "answer",
            "session",
            "turns",
            "stopped",
            "model",
            "duration_ms",
            "tool_calls",
            "usage",
            "enforcement",
            "command_ms",
            "output_bytes",
        ],
        "the field order is fixed and later fields only append (§15): {json}"
    );
}

#[test]
fn a_run_that_answered_reports_its_text_and_how_it_ended() {
    let json = reported("hello", Says("Hello world"), Costs).json();
    let value: serde_json::Value = serde_json::from_str(&json).expect("one object");

    assert_eq!(value["answer"], "Hello world", "{json}");
    assert_eq!(value["stopped"], "final", "{json}");
    assert_eq!(value["turns"], 1, "{json}");
    assert_eq!(
        value["session"], "019fd6ff-2b7a-7c31-9d6c-0b6b0e0f4a11",
        "the GUID a script joins the session directory by: {json}"
    );
    assert_eq!(value["model"], "claude-sonnet-4-6", "{json}");
    assert_eq!(value["duration_ms"], 1234, "{json}");
    assert_eq!(value["enforcement"], "bubblewrap", "{json}");
}

#[test]
fn a_run_that_failed_leaves_the_answer_out_rather_than_writing_null() {
    let json = reported("hello", Fails, Costs).json();

    assert!(
        !json.contains("null"),
        "a field the run knows nothing about is left out, never `null` (§15): {json}"
    );
    assert!(
        !keys(&json).contains(&String::from("answer")),
        "a failed run has no answer to report: {json}"
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&json).expect("one object")["stopped"],
        "error",
        "{json}"
    );
}

#[test]
fn usage_no_wire_reported_is_left_out_and_usage_reported_is_reported_back() {
    let silent = reported("hello", Says("hi"), Costs).json();
    assert!(
        !keys(&silent).contains(&String::from("usage")),
        "a token count nobody reported is not a fact the report can invent: {silent}"
    );

    let mut run = Run::new("hello", Says("hi"), Costs);
    let mut quiet = Quiet;
    run.drive(&mut quiet);
    let mut told = facts("019fd6ff-2b7a-7c31-9d6c-0b6b0e0f4a11", "claude-sonnet-4-6");
    told.usage = Usage {
        input: Some(11),
        output: Some(7),
        ..Usage::default()
    };
    let json = Outcome::of(&run, told).expect("a stopped run").json();
    let value: serde_json::Value = serde_json::from_str(&json).expect("one object");

    assert_eq!(value["usage"]["input"], 11, "{json}");
    assert_eq!(value["usage"]["output"], 7, "{json}");
    assert!(
        value["usage"].get("total").is_none(),
        "a count the wire never gave is left out of the usage too: {json}"
    );
}

#[test]
fn each_stop_reason_reports_the_exit_code_the_spec_gives_it() {
    let answered = reported("hello", Says("hi"), Costs);
    assert_eq!(answered.exit_code(), 0, "an answer");
    assert_eq!(*answered.stopped(), Stop::Final);

    let failed = reported("hello", Fails, Costs);
    assert_eq!(failed.exit_code(), 1, "a model or provider error");
    assert!(matches!(failed.stopped(), Stop::Error(_)));

    let capped = {
        let mut run =
            Run::new("hello", Loopy, Costs).limits(Limits::new(3, Duration::MAX, u64::MAX));
        let mut quiet = Quiet;
        run.drive(&mut quiet);
        Outcome::of(&run, facts("id", "m")).expect("a stopped run")
    };
    assert_eq!(capped.exit_code(), 100, "the turn cap");

    let spent = {
        let mut run =
            Run::new("hello", Loopy, Costs).limits(Limits::new(9, Duration::ZERO, u64::MAX));
        let mut quiet = Quiet;
        run.drive(&mut quiet);
        Outcome::of(&run, facts("id", "m")).expect("a stopped run")
    };
    assert_eq!(spent.exit_code(), 101, "a budget");

    let cancelled = {
        let mut run = Run::new("hello", Says("hi"), Costs);
        run.canceller().now();
        let mut quiet = Quiet;
        run.drive(&mut quiet);
        Outcome::of(&run, facts("id", "m")).expect("a stopped run")
    };
    assert_eq!(cancelled.exit_code(), 130, "the user's own word");
}

#[test]
fn a_report_counts_the_calls_a_run_made_and_what_they_cost() {
    let json = reported("hello", Loopy, Costs).json();
    let value: serde_json::Value = serde_json::from_str(&json).expect("one object");
    let turns = value["turns"].as_u64().expect("a turn count");

    assert!(
        turns >= 2,
        "a run that keeps asking for a command takes more than one turn: {json}"
    );
    assert_eq!(
        value["tool_calls"], turns,
        "one call per turn, counted rather than guessed at: {json}"
    );
    assert_eq!(
        value["command_ms"],
        turns * 5,
        "the command wall clock the run spent: {json}"
    );
    assert_eq!(
        value["output_bytes"],
        turns * 64,
        "the output the run fed back: {json}"
    );
}

#[test]
fn the_report_is_one_compact_line_with_nothing_after_it() {
    let json = reported("hello", Says("hi"), Costs).json();

    assert!(
        !json.contains('\n'),
        "one line, and a script may split on it: {json}"
    );
    assert!(
        !json.contains(": "),
        "compact, with no space after a colon: {json}"
    );
    assert!(
        !json.contains(", "),
        "compact, with no space after a comma: {json}"
    );
    assert!(json.starts_with('{') && json.ends_with('}'), "{json}");
}
