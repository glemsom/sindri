//! The `open` tool's contract with the model: one target per call, the refusals
//! made before the host is asked anything, and a result that says what the opener
//! did rather than what a human saw. The record and the row that read the same
//! call are proven beside the two that write them.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use std::sync::{Arc, Mutex};

use sindri_core::agent::{BashTool, Limits, OpenTool, Opened, Opener, Tools};
use sindri_core::provider::{Asked, Call, ToolOutcome};
use support::{Scripted, fixtures};

/// The host's opener, faked: every target it was asked for, and the one answer
/// it gives. A test that says it was never asked reads an empty list.
struct Fake {
    seen: Mutex<Vec<String>>,
    answer: Opened,
}

impl Fake {
    fn answering(answer: Opened) -> Arc<Fake> {
        Arc::new(Fake {
            seen: Mutex::default(),
            answer,
        })
    }

    fn asked(&self) -> Vec<String> {
        self.seen.lock().expect("the opener's log").clone()
    }
}

impl Opener for Fake {
    fn open(&self, target: &str) -> Opened {
        self.seen
            .lock()
            .expect("the opener's log")
            .push(target.to_string());
        self.answer.clone()
    }
}

/// A run with the third tool, over an opener a test controls.
fn tools(opener: Arc<Fake>) -> Tools {
    let (_workspace, _session_temp, policy) = fixtures();
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    Tools::new(bash, None, Some(OpenTool::new(opener)))
}

/// A run with no browser anywhere near it, as batch mode builds one.
fn windowless() -> Tools {
    let (_workspace, _session_temp, policy) = fixtures();
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    Tools::new(bash, None, None)
}

/// The call the model makes to `open`, with the arguments it sent.
fn call(arguments: &[(&str, &str)]) -> Call {
    let sent = arguments
        .iter()
        .map(|(key, value)| ((*key).to_string(), serde_json::json!(value)));
    Call::new("call_1", "open", Ok(serde_json::Map::from_iter(sent)))
}

fn open(target: &str) -> Call {
    call(&[("url", target)])
}

#[test]
fn an_https_url_is_handed_to_the_host_and_the_result_reads_started() {
    let opener = Fake::answering(Opened::Started);
    let tools = tools(Arc::clone(&opener));
    let call = open("https://example.com/report.html");

    let outcome = tools.call(&call, &Default::default()).expect("a call runs");

    assert_eq!(
        opener.asked(),
        vec!["https://example.com/report.html".to_string()],
        "the target reaches the host as the model wrote it"
    );
    assert_eq!(
        call.asked(),
        Asked::Open {
            target: "https://example.com/report.html".to_string(),
        }
    );
    assert_eq!(outcome.facts.outcome.as_deref(), Some("started"));
    assert!(
        !outcome.is_error,
        "a window that refused nothing is no error"
    );
    assert!(
        outcome.content.contains("outcome: started")
            && outcome
                .content
                .contains("target: https://example.com/report.html"),
        "the result names the target it handed over: {}",
        outcome.content
    );
    assert!(
        !outcome.content.contains("the human saw") && !outcome.content.contains("opened in"),
        "a result never claims a human looked: {}",
        outcome.content
    );
}

#[test]
fn an_http_url_is_a_target_as_much_as_an_https_one() {
    let opener = Fake::answering(Opened::Started);
    let tools = tools(Arc::clone(&opener));

    let outcome = tools
        .call(&open("http://127.0.0.1:8080/x"), &Default::default())
        .expect("a call runs");

    assert_eq!(opener.asked(), vec!["http://127.0.0.1:8080/x".to_string()]);
    assert_eq!(outcome.facts.outcome.as_deref(), Some("started"));
}

#[test]
fn an_opener_that_fails_inside_the_window_is_failed_with_its_reason() {
    let opener = Fake::answering(Opened::Failed {
        reason: "the opener exited 4".to_string(),
    });
    let tools = tools(opener);

    let outcome = tools
        .call(&open("https://example.com/x"), &Default::default())
        .expect("a call runs");

    assert_eq!(outcome.facts.outcome.as_deref(), Some("failed"));
    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("outcome: failed")
            && outcome.content.contains("problem: the opener exited 4"),
        "the host's own words, not a guess at them: {}",
        outcome.content
    );
}

#[test]
fn a_scheme_that_is_not_http_is_refused_naming_the_scheme_it_got() {
    for (target, scheme) in [
        ("file:///etc/passwd", "file:"),
        ("ftp://example.com/x", "ftp:"),
        ("mailto:someone@example.com", "mailto:"),
    ] {
        let opener = Fake::answering(Opened::Started);
        let tools = tools(Arc::clone(&opener));

        let outcome = tools
            .call(&open(target), &Default::default())
            .expect("a call is answered even when no tool runs it");

        assert!(
            opener.asked().is_empty(),
            "{target} must never reach the host: {:?}",
            opener.asked()
        );
        assert!(outcome.is_error, "{target}: {}", outcome.content);
        assert!(
            outcome.content.contains(scheme),
            "the refusal names the scheme it got, `{scheme}`: {}",
            outcome.content
        );
    }
}

#[test]
fn a_call_naming_neither_argument_is_refused_before_the_opener_runs() {
    let opener = Fake::answering(Opened::Started);
    let tools = tools(Arc::clone(&opener));

    let outcome = tools
        .call(&call(&[]), &Default::default())
        .expect("a call is answered");

    assert!(opener.asked().is_empty(), "nothing was opened");
    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("names no target"),
        "the refusal says what was missing: {}",
        outcome.content
    );
}

#[test]
fn a_call_naming_both_arguments_is_refused_before_the_opener_runs() {
    let opener = Fake::answering(Opened::Started);
    let tools = tools(Arc::clone(&opener));

    let outcome = tools
        .call(
            &call(&[("url", "https://example.com/x"), ("path", "report.html")]),
            &Default::default(),
        )
        .expect("a call is answered");

    assert!(opener.asked().is_empty(), "nothing was opened");
    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("names two targets"),
        "the refusal says what was extra: {}",
        outcome.content
    );
}

#[test]
fn a_path_is_refused_until_the_guard_that_reads_files_exists() {
    let opener = Fake::answering(Opened::Started);
    let tools = tools(Arc::clone(&opener));

    let outcome = tools
        .call(&call(&[("path", "report.html")]), &Default::default())
        .expect("a call is answered");

    assert!(
        opener.asked().is_empty(),
        "a path no rule has read never reaches the host's mimetype handler"
    );
    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("does not open a `path`"),
        "the refusal names the ground it stopped on: {}",
        outcome.content
    );
}

#[test]
fn a_run_with_no_browser_is_never_offered_the_tool() {
    let specs = windowless().specs();

    assert_eq!(
        specs.len(),
        1,
        "batch mode's schema is `bash` alone: {specs:?}"
    );
    assert_eq!(specs[0].name, "bash");

    // And a model that names it anyway hears the tool is absent here, which is a
    // fact about the run's toolset and not about the target.
    let outcome: ToolOutcome = windowless()
        .call(&open("https://example.com/x"), &Default::default())
        .expect("a call is answered");
    assert!(outcome.is_error);
    assert!(
        outcome
            .content
            .contains("there is no `open` tool in this run"),
        "{}",
        outcome.content
    );
}

#[test]
fn the_description_teaches_the_two_shapes_and_for_whom_the_window_opens() {
    let (_workspace, _session_temp, policy) = fixtures();
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    let tools = Tools::new(
        bash,
        None,
        Some(OpenTool::new(Fake::answering(Opened::Started))),
    );
    let specs = tools.specs();
    let spec = specs.last().expect("the third tool is registered");

    assert_eq!(spec.name, "open");
    let description = &spec.description;
    for taught in [
        "for them, not for you",
        "`bash` is how you read",
        "`url`",
        "`path`",
        "`started`",
        "`failed:`",
        "a human looked",
        "far side of a process boundary",
    ] {
        assert!(
            description.contains(taught),
            "the description teaches `{taught}`: {description}"
        );
    }
    let parameters = &spec.parameters;
    assert_eq!(parameters["type"], "object");
    assert!(
        parameters["properties"]["url"].is_object() && parameters["properties"]["path"].is_object(),
        "the two argument shapes are the two properties: {parameters}"
    );
    assert!(
        parameters.get("required").is_none(),
        "exactly one target is the parse's rule, not the schema's: {parameters}"
    );
}
