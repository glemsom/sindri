//! The `open` tool's contract with the model: one target per call, the refusals
//! made before the host is asked anything, and a result that says what the opener
//! did rather than what a human saw. The record and the row that read the same
//! call are proven beside the two that write them. A `file:` URI, which is the
//! `path` argument in another spelling, is proven in `open_file_uri.rs`.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use std::sync::Arc;

use sindri_core::agent::{BashTool, Limits, OpenTool, Opened, Tools};
use sindri_core::provider::{Asked, Target, ToolOutcome};
use support::open::{Fake, READABLE, Run, call, open, tools, windowless};
use support::{Scripted, fixtures};
use tempfile::TempDir;

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
            target: Target::Url("https://example.com/report.html".to_string()),
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
fn every_scheme_but_http_https_and_file_is_refused_naming_the_scheme_it_got() {
    // `file:` is not this rule's to refuse: it is the `path` argument in another
    // spelling, so it reduces to a path and is guarded as one — which
    // `open_file_uri.rs` proves beside the identical refusal a path gets.
    for (target, scheme) in [
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
    assert!(
        outcome.content.contains("file:"),
        "and names every spelling a target may be sent in, `file:` beside the two URLs: {}",
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
fn a_file_in_the_writable_set_is_handed_over_as_its_canonical_path() {
    let run = Run::new();
    let report = run.report("notes/report.html");

    let outcome = run.shown("notes/report.html");

    assert_eq!(
        run.asked(),
        vec![report.display().to_string()],
        "the host is asked for the resolved file, never the string the model typed"
    );
    assert_eq!(outcome.facts.outcome.as_deref(), Some("started"));
    assert!(!outcome.is_error, "a file the guard admitted is no error");
    assert!(
        outcome
            .content
            .contains(&format!("target: {}\n", report.display())),
        "the result echoes the absolute canonical path, so the model can quote the one a \
         human could paste: {}",
        outcome.content
    );
    assert!(
        !outcome.content.contains("target: notes/"),
        "and never the relative path it was given: {}",
        outcome.content
    );
}

#[test]
fn a_file_in_the_session_temp_is_admitted_too() {
    let run = Run::apart();
    let report = run.write(&run.session_temp, "chart.svg");

    let outcome = run.shown(&report.display().to_string());

    assert_eq!(
        run.asked(),
        vec![report.display().to_string()],
        "the session temp is the other place a run may write, and a file it put there is a \
         file a human may be shown: {}",
        outcome.content
    );
    assert_eq!(outcome.facts.outcome.as_deref(), Some("started"));
}

#[test]
fn a_path_outside_the_writable_set_and_the_session_temp_is_refused() {
    let run = Run::new();
    let outside = TempDir::new().expect("a tree the run may not write");
    let report = run.write(outside.path(), "report.html");

    let outcome = run.shown(&report.display().to_string());

    assert!(
        run.asked().is_empty(),
        "a file outside the run's own writes never reaches the host's handler"
    );
    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("outside the writable set")
            && outcome.content.contains(&report.display().to_string()),
        "the refusal names the ground it stopped on and the file it refused: {}",
        outcome.content
    );
}

#[test]
fn a_symlink_that_leaves_the_writable_set_is_refused_without_naming_where_it_led() {
    let run = Run::new();
    let outside = TempDir::new().expect("a tree the run may not write");
    let secret = run.write(outside.path(), "report.html");
    let link = run.workspace.path().join("escape.html");
    std::os::unix::fs::symlink(&secret, &link).expect("a link out of the writable set");

    let outcome = run.shown("escape.html");

    assert!(
        run.asked().is_empty(),
        "a write permission is not laundered into a file shown to a human"
    );
    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("escape.html"),
        "the refusal names the link it would not follow: {}",
        outcome.content
    );
    assert!(
        !outcome.content.contains(&secret.display().to_string()),
        "and never echoes a path outside what a run may read, or `open` would answer whether \
         a file the run cannot read exists: {}",
        outcome.content
    );
}

#[test]
fn a_link_whose_resolved_file_is_not_one_a_browser_reads_is_refused() {
    let run = Run::new();
    for (shows_as, really) in [("report.html", "payload.desktop"), ("notes.txt", "notes")] {
        let payload = run.write(run.workspace.path(), really);
        let link = run.workspace.path().join(shows_as);
        std::os::unix::fs::symlink(&payload, &link).expect("a link to the file it really is");

        let outcome = run.shown(shows_as);

        assert!(
            run.asked().is_empty(),
            "`{shows_as}` is a `{really}` underneath, and the host is handed what it resolves to, \
             so no `.desktop` may reach a handler: {}",
            outcome.content
        );
        assert!(outcome.is_error);
        assert!(
            outcome.content.contains(READABLE),
            "the refusal names every kind that would have opened: {}",
            outcome.content
        );
    }
}

#[test]
fn a_path_that_names_nothing_is_refused() {
    let run = Run::new();

    let outcome = run.shown("missing.html");

    assert!(run.asked().is_empty(), "nothing that is not there is shown");
    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("missing.html") && outcome.content.contains("does not exist"),
        "the refusal says there is no such file: {}",
        outcome.content
    );
}

#[test]
fn a_directory_is_refused_because_a_browser_is_handed_files() {
    let run = Run::new();
    std::fs::create_dir(run.workspace.path().join("pages.html")).expect("a directory");

    let outcome = run.shown("pages.html");

    assert!(run.asked().is_empty(), "a directory is not a file to open");
    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("not a regular file"),
        "the refusal names what the path is instead: {}",
        outcome.content
    );
}

#[test]
fn an_extension_no_browser_reads_is_refused_naming_the_whole_set() {
    let run = Run::new();

    for given in ["install.desktop", "script.sh", "Makefile", "notes.rst"] {
        let outcome = run.shown(given);

        assert!(
            run.asked().is_empty(),
            "`{given}` must never reach the host's mimetype guesser"
        );
        assert!(outcome.is_error);
        assert!(
            outcome.content.contains(READABLE),
            "the refusal names every extension that would have opened: {}",
            outcome.content
        );
        assert!(
            outcome.content.contains(given),
            "and the path it refused: {}",
            outcome.content
        );
    }
}

#[test]
fn an_extension_is_read_whatever_its_case() {
    let run = Run::new();
    let report = run.report("index.HTML");

    let outcome = run.shown("index.HTML");

    assert_eq!(
        run.asked(),
        vec![report.display().to_string()],
        "an upper-case spelling is the same kind of file: {}",
        outcome.content
    );
    assert!(!outcome.is_error);
}

#[test]
fn a_path_is_one_argument_never_a_word_a_shell_reads() {
    let run = Run::new();
    let name = "report;touch hoisted.html";
    let report = run.report(name);

    let outcome = run.shown(name);

    assert_eq!(
        run.asked(),
        vec![report.display().to_string()],
        "the path arrives as one target, whatever words it holds: {}",
        outcome.content
    );
    for hoisted in ["hoisted", "hoisted.html"] {
        assert!(
            !run.workspace.path().join(hoisted).exists(),
            "no shell read the path: `{hoisted}` was not created"
        );
    }
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
    let (workspace, _session_temp, policy) = fixtures();
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    let tools = Tools::new(
        bash,
        None,
        Some(OpenTool::new(
            Fake::answering(Opened::Started),
            policy.clone(),
            workspace
                .path()
                .canonicalize()
                .expect("a canonical workspace"),
        )),
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
        "`file:` URI",
        "`started`",
        "`failed:`",
        "a human looked",
        "far side of a process boundary",
        "a run could have written",
        "writable set",
        "browser reads",
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
