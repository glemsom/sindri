#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken record reports itself"
)]

//! The session record, proven where the record lives: a directory on
//! disk a run writes as it goes. No process, no sandbox — what is under test is
//! what a user finds after the fact.

use std::time::Duration;

use sindri_core::agent::{Report, Stopped};
use sindri_core::exec::StatusSink;
use sindri_core::paths::SessionDir;
use sindri_core::provider::{Call, ToolOutcome, Usage};
use sindri_core::record::{Entry, Recorder, Redaction, ShowError, Start, Stored};
use sindri_core::sandbox::Enforcement;
use tempfile::TempDir;

const KEY: &str = "sk-ant-test-not-a-real-key";

fn recording(enforcement: Enforcement) -> (TempDir, SessionDir, Recorder) {
    let root = TempDir::new().expect("state dir");
    let session = SessionDir::at(root.path().join("sessions/session-1"));
    let recorder = Recorder::start(
        &session,
        Start {
            provider: "anthropic".to_string(),
            model: "claude-mock".to_string(),
            enforcement,
        },
        Redaction::new([KEY.to_string()]),
    )
    .expect("start the record");
    (root, session, recorder)
}

fn call(id: &str, command: &str) -> Call {
    Call::new(
        id,
        "bash",
        Ok(serde_json::Map::from_iter([(
            "command".to_string(),
            serde_json::json!(command),
        )])),
    )
}

fn report() -> Report {
    Report {
        answer: "Twelve files.".to_string(),
        turns: 2,
        stopped: Stopped::Final,
        model: Some("claude-mock".to_string()),
        tool_calls: 1,
        usage: Usage {
            input_tokens: Some(35),
            output_tokens: Some(12),
            cached_input_tokens: None,
            reasoning_tokens: None,
            total_tokens: Some(47),
        },
        duration: Duration::from_millis(1234),
    }
}

fn everything_written(dir: &std::path::Path) -> String {
    let mut all = String::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                all.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    all
}

#[test]
fn a_record_names_the_run_the_sandbox_and_where_it_got_to() {
    let (_root, session, recorder) = recording(Enforcement::Bubblewrap);

    recorder.user("how many files?");
    recorder.assistant(1, "Looking.", Some("counting"));
    recorder.tool_call(1, &call("call_1", "ls | wc -l"));
    recorder.tool_result(
        1,
        &ToolOutcome::new(
            &call("call_1", "ls | wc -l"),
            "outcome: exited\n".to_string(),
            false,
        ),
    );
    recorder.assistant(2, "Twelve files.", None);
    recorder.finish(&report()).expect("finish the record");

    let stored = Stored::load_in(&session).expect("read the record back");
    assert_eq!(stored.meta.provider, "anthropic");
    assert_eq!(stored.meta.model, "claude-mock");
    assert_eq!(stored.meta.enforcement, "bubblewrap");
    assert_eq!(stored.meta.stopped, "final");
    assert_eq!(stored.meta.turns, Some(2));
    assert_eq!(stored.meta.tool_calls, Some(1));
    assert_eq!(stored.meta.duration_ms, Some(1234));
    assert_eq!(
        stored.meta.usage.as_ref().and_then(|u| u.total_tokens),
        Some(47)
    );
    assert!(stored.meta.ended_ms.is_some(), "a finished run says when");

    assert!(
        matches!(&stored.entries[0], Entry::User { text } if text == "how many files?"),
        "the prompt comes first: {:?}",
        stored.entries
    );
    assert!(
        matches!(&stored.entries[1], Entry::Assistant { turn: 1, text, reasoning }
            if text == "Looking." && reasoning.as_deref() == Some("counting")),
        "{:?}",
        stored.entries
    );
    assert!(
        matches!(&stored.entries[2], Entry::ToolCall { call_id, command, .. }
            if call_id == "call_1" && command.as_deref() == Some("ls | wc -l")),
        "{:?}",
        stored.entries
    );
    assert!(
        matches!(&stored.entries[3], Entry::ToolResult { call_id, content, .. }
            if call_id == "call_1" && content.contains("outcome: exited")),
        "{:?}",
        stored.entries
    );
    assert_eq!(stored.entries.len(), 5, "one entry per thing that happened");

    let transcript =
        std::fs::read_to_string(session.root().join("transcript.jsonl")).expect("the transcript");
    assert_eq!(
        transcript.lines().count(),
        5,
        "one JSON object per line, no blank lines between them"
    );
    assert!(
        session.root().join("session.json").is_file(),
        "the metadata is a file of its own"
    );
    assert!(
        session.root().join("status.json").is_file(),
        "the sandbox's own documents have a home even before there are any"
    );
}

#[test]
fn an_unsafe_run_says_so_in_its_record() {
    let (_root, session, recorder) = recording(Enforcement::YoloUnsafe);
    recorder.user("probe");
    recorder.finish(&report()).expect("finish the record");

    let stored = Stored::load_in(&session).expect("read the record back");
    assert_eq!(
        stored.meta.enforcement, "unsafe",
        "a record must never claim a sandbox a run did not have"
    );
    assert!(
        stored.render().contains("unsafe"),
        "and the readable transcript says it too: {}",
        stored.render()
    );
}

#[test]
fn a_credential_never_reaches_the_record() {
    let (_root, session, recorder) = recording(Enforcement::Bubblewrap);
    let leaky = format!("curl -H 'x-api-key: {KEY}' example.invalid");

    recorder.user(&format!("what is {KEY}?"));
    recorder.tool_call(1, &call("call_1", &leaky));
    recorder.tool_result(
        1,
        &ToolOutcome::new(&call("call_1", &leaky), leaky.clone(), false),
    );
    recorder.failed(&format!("the provider refused {KEY}")).ok();
    recorder.finish(&report()).expect("finish the record");

    let written = everything_written(session.root());
    assert!(
        !written.contains(KEY),
        "the credential must be redacted before anything is written: {written}"
    );
    assert!(
        written.contains("[redacted]"),
        "and the redaction must be visible, not the text quietly dropped: {written}"
    );
}

#[test]
fn a_run_that_never_finished_still_reads() {
    let (_root, session, recorder) = recording(Enforcement::Bubblewrap);
    recorder.user("how many files?");
    recorder.tool_call(1, &call("call_1", "sleep 600"));
    // No `finish`: a crash or a `kill -9` looks exactly like this.
    drop(recorder);

    let stored = Stored::load_in(&session).expect("a half-written record still reads");
    assert_eq!(
        stored.meta.stopped, "running",
        "a record that never ended says so rather than guessing"
    );
    assert!(stored.meta.ended_ms.is_none(), "there is no end to report");
    assert_eq!(
        stored.entries.len(),
        2,
        "everything written up to the crash"
    );
    assert!(
        stored.render().contains("sleep 600"),
        "the command it was running is the point: {}",
        stored.render()
    );
}

#[test]
fn the_sandbox_documents_are_written_as_they_arrive() {
    let (_root, session, recorder) = recording(Enforcement::Bubblewrap);

    recorder.document(1, r#"{"child-pid":4711}"#);
    recorder.document(1, r#"{"exit-code":0}"#);
    // A document bwrap wrote that is not JSON is still evidence, not a loss.
    recorder.document(2, "not a document");

    let status = std::fs::read_to_string(session.root().join("status.json")).expect("the status");
    let lines: Vec<serde_json::Value> = status
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect();
    assert_eq!(lines.len(), 3, "every document is kept: {status}");
    assert_eq!(lines[0]["call"], 1);
    assert_eq!(lines[0]["document"]["child-pid"], 4711);
    assert_eq!(lines[1]["document"]["exit-code"], 0);
    assert_eq!(lines[2]["line"], "not a document");
}

#[test]
fn a_record_that_could_not_be_started_says_why() {
    let root = TempDir::new().expect("state dir");
    let file = root.path().join("not-a-directory");
    std::fs::write(&file, "in the way").expect("write the obstruction");

    let refused = Recorder::start(
        &SessionDir::at(file),
        Start {
            provider: "anthropic".to_string(),
            model: "claude-mock".to_string(),
            enforcement: Enforcement::Bubblewrap,
        },
        Redaction::default(),
    );
    assert!(
        refused.is_err(),
        "a record that cannot be written is not a run"
    );
}

/// A record whose last line a crash cut short: `kill -9` lands mid-`writeln!`,
/// and that record is exactly the one a debugger wants to open.
#[test]
fn a_transcript_cut_short_still_reads_and_counts_the_line_it_lost() {
    let (_root, session, recorder) = recording(Enforcement::Bubblewrap);
    recorder.user("how many files?");
    recorder.tool_call(1, &call("call_1", "sleep 600"));
    drop(recorder);

    let path = session.root().join("transcript.jsonl");
    let mut transcript = std::fs::read_to_string(&path).expect("the transcript");
    transcript.push_str(r#"{"type":"tool_result","turn":1,"cont"#);
    std::fs::write(&path, transcript).expect("the cut transcript");

    let stored = Stored::load_in(&session).expect("a cut line is not a corrupt record");
    assert_eq!(stored.unread, 1, "the one line the crash cut in two");
    assert_eq!(stored.entries.len(), 2, "everything before the cut is kept");
    let rendered = stored.render();
    assert!(
        rendered.contains("1 line could not be read"),
        "the reader is told what the record lost: {rendered}"
    );
    assert!(
        rendered.contains("sleep 600"),
        "and still reads what it holds: {rendered}"
    );
}

/// The other end of the same file: a run killed before it wrote a whole first
/// line leaves a header, no entries, and the count — never a refusal.
#[test]
fn a_transcript_whose_first_line_is_broken_renders_a_header_and_the_count() {
    let (_root, session, _recorder) = recording(Enforcement::Bubblewrap);
    std::fs::write(
        session.root().join("transcript.jsonl"),
        r#"{"type":"user","tex"#,
    )
    .expect("a broken first line");

    let stored = Stored::load_in(&session).expect("a broken first line is not a corrupt record");
    assert!(stored.entries.is_empty(), "there was nothing before it");
    assert_eq!(stored.unread, 1);
    let rendered = stored.render();
    assert!(
        rendered.contains("1 line could not be read"),
        "the count stands in for the entries: {rendered}"
    );
    assert!(
        !rendered.contains("--- user ---"),
        "and no entry is invented: {rendered}"
    );
}

/// The metadata is the record: without it there is nothing to read, and that is
/// the one corruption that is still a refusal.
#[test]
fn metadata_that_does_not_parse_is_still_corrupt() {
    let (_root, session, recorder) = recording(Enforcement::Bubblewrap);
    recorder.user("how many files?");
    drop(recorder);
    std::fs::write(session.root().join("session.json"), "{\"id\"").expect("broken metadata");

    let refused = Stored::load_in(&session)
        .err()
        .expect("without the metadata there is no record");
    assert!(
        matches!(refused, ShowError::Corrupt { .. }),
        "and the refusal says so: {refused:?}"
    );
}

/// What bubblewrap said, verbatim, where a post-mortem can read it.
#[test]
fn the_sandbox_status_is_rendered_under_its_own_heading() {
    let (_root, session, recorder) = recording(Enforcement::Bubblewrap);
    recorder.user("how many files?");
    recorder.document(1, r#"{"child-pid":4711}"#);
    recorder.document(2, "not a document");
    drop(recorder);

    let stored = Stored::load_in(&session).expect("the record reads");
    assert_eq!(
        stored.status.len(),
        2,
        "one line per document, as they were written: {:?}",
        stored.status
    );
    let rendered = stored.render();
    assert!(
        rendered.contains("--- sandbox status ---"),
        "the status has a heading of its own: {rendered}"
    );
    assert!(
        rendered.contains(r#"{"child-pid":4711}"#),
        "and its lines are verbatim, not ours to type: {rendered}"
    );
    assert!(
        rendered.find("--- sandbox status ---") > rendered.find("--- user ---"),
        "the entries come first, the status after them: {rendered}"
    );
}

/// A record with neither the transcript nor the status still reads: only the
/// metadata is required.
#[test]
fn a_record_with_no_transcript_and_no_status_still_reads() {
    let (_root, session, _recorder) = recording(Enforcement::Bubblewrap);
    std::fs::remove_file(session.root().join("transcript.jsonl")).expect("no transcript");
    std::fs::remove_file(session.root().join("status.json")).expect("no status");

    let stored = Stored::load_in(&session).expect("a record is mostly metadata");
    assert!(stored.entries.is_empty());
    assert!(stored.status.is_empty());
    assert_eq!(stored.unread, 0);
    let rendered = stored.render();
    assert!(
        !rendered.contains("could not be read"),
        "a short record is not a loss: {rendered}"
    );
    assert!(
        !rendered.contains("--- sandbox status ---"),
        "and a record without a status says nothing about it: {rendered}"
    );
}
