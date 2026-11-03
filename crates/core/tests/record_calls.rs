//! The tool-call line of the record: the call as the loop read it, never a
//! string a reader has to parse twice. A skill and a command share one entry,
//! and the skill's name is written the way the call was made.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken record reports itself"
)]

mod support;

use sindri_core::paths::SessionDir;
use sindri_core::provider::Asked;
use sindri_core::provider::Call;
use sindri_core::record::{Entry, Recorder, Redaction, Stored};
use sindri_core::sandbox::Enforcement;
use support::start;
use tempfile::TempDir;

fn recording() -> (TempDir, SessionDir, Recorder) {
    let root = TempDir::new().expect("state dir");
    let session = SessionDir::at(root.path().join("sessions/session-1"));
    let recorder = Recorder::start(
        &session,
        start("anthropic", "claude-mock", Enforcement::Bubblewrap),
        Redaction::default(),
    )
    .expect("start the record");
    (root, session, recorder)
}

#[test]
fn a_skill_call_is_written_as_the_call_it_was() {
    let (_root, session, recorder) = recording();
    let loaded = Call::new(
        "call_1",
        "skill",
        Ok(serde_json::Map::from_iter([(
            "name".to_string(),
            serde_json::json!("house-style"),
        )])),
    );

    recorder.user("say something");
    recorder.tool_call(1, &loaded);

    let stored = Stored::load_in(&session).expect("a record still being written reads");
    assert!(
        matches!(
            &stored.entries[1],
            Entry::ToolCall { asked: Asked::Skill { name }, .. } if name == "house-style"
        ),
        "the record holds the request, not a string that spells it: {:?}",
        stored.entries
    );
    let shown = stored.render();
    assert!(
        shown.contains("--- skill (turn 1) call_1 ---") && shown.contains("load house-style"),
        "and a reader is told which skill was loaded: {shown}"
    );
}

/// A call to the third tool: one target, as the model wrote it.
fn opening(id: &str, target: &str) -> Call {
    Call::new(
        id,
        "open",
        Ok(serde_json::Map::from_iter([(
            "url".to_string(),
            serde_json::json!(target),
        )])),
    )
}

#[test]
fn an_open_call_is_written_as_the_call_it_was() {
    let (_root, session, recorder) = recording();
    let opened = opening("call_2", "https://example.com/report.html");

    recorder.user("show me the report");
    recorder.tool_call(1, &opened);

    let stored = Stored::load_in(&session).expect("a record still being written reads");
    assert!(
        matches!(
            &stored.entries[1],
            Entry::ToolCall { asked: Asked::Open { target }, .. }
                if target.as_str() == "https://example.com/report.html"
        ),
        "the record keeps the target as a field, not a string to parse twice: {:?}",
        stored.entries
    );
    let shown = stored.render();
    assert!(
        shown.contains("open (turn 1) call_2")
            && shown.contains("open https://example.com/report.html"),
        "a reader of the record is told what was opened: {shown}"
    );
    assert!(
        stored
            .markdown()
            .contains("open https://example.com/report.html"),
        "and the export says it in the same words: {}",
        stored.markdown()
    );
}

#[test]
fn an_open_target_is_redacted_in_the_record_as_a_command_is() {
    let key = "sk-ant-test-not-a-real-key";
    let root = TempDir::new().expect("state dir");
    let session = SessionDir::at(root.path().join("sessions/session-1"));
    let recorder = Recorder::start(
        &session,
        start("anthropic", "claude-mock", Enforcement::Bubblewrap),
        Redaction::new([key.to_string()]),
    )
    .expect("start the record");
    let leaky = format!("https://example.com/{key}/report");

    recorder.user("show me the page");
    recorder.tool_call(1, &opening("call_2", &leaky));

    let stored = Stored::load_in(&session).expect("a record still being written reads");
    let written = stored.render();
    assert!(
        !written.contains(key),
        "a target is a string the model composed, and it is redacted like a command: {written}"
    );
    assert!(
        written.contains("https://example.com/[redacted]/report"),
        "the rest of the URL is kept, so a reader can still see where it pointed: {written}"
    );
}
