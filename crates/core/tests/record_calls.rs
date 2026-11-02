//! The tool-call line of the record: the call as the loop read it, never a
//! string a reader has to parse twice. A skill and a command share one entry,
//! and the skill's name is written the way the call was made.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken record reports itself"
)]

use sindri_core::paths::SessionDir;
use sindri_core::provider::Asked;
use sindri_core::provider::Call;
use sindri_core::record::{Entry, Recorder, Redaction, Start, Stored};
use sindri_core::sandbox::Enforcement;
use tempfile::TempDir;

fn recording() -> (TempDir, SessionDir, Recorder) {
    let root = TempDir::new().expect("state dir");
    let session = SessionDir::at(root.path().join("sessions/session-1"));
    let recorder = Recorder::start(
        &session,
        Start::of(
            "anthropic".to_string(),
            "claude-mock".to_string(),
            Enforcement::Bubblewrap,
        ),
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
