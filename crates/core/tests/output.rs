//! The shape of the JSON output (ADR-0004), pinned byte for byte: the field
//! order, what absent data does, and every word `stopped` can take. A parser
//! written against this shape must keep working when later fields append, so
//! the shape is asserted as an exact literal rather than by inspection.

use std::time::Duration;

use sindri_core::agent::{Report, Stopped};
use sindri_core::output::to_json;
use sindri_core::provider::Usage;

/// A session GUID, shaped like the kernel's own but belonging to nothing.
const SESSION: &str = "0f4a2f2e-6a3b-4b4d-9a2c-9d1f2a3b4c5d";

#[test]
fn the_shape_is_pinned_byte_for_byte() {
    let report = Report {
        answer: "A rename, in two files.".to_string(),
        turns: 2,
        stopped: Stopped::Final,
        model: Some("claude-sonnet-4-6".to_string()),
        tool_calls: 1,
        usage: Usage {
            input_tokens: Some(1234),
            output_tokens: Some(56),
            cached_input_tokens: Some(1000),
            reasoning_tokens: None,
            total_tokens: Some(1290),
        },
        duration: Duration::from_millis(4321),
    };

    assert_eq!(
        to_json(&report, SESSION),
        concat!(
            r#"{"answer":"A rename, in two files.","#,
            r#""session":"0f4a2f2e-6a3b-4b4d-9a2c-9d1f2a3b4c5d","#,
            r#""turns":2,"#,
            r#""stopped":"final","#,
            r#""model":"claude-sonnet-4-6","#,
            r#""duration_ms":4321,"#,
            r#""tool_calls":1,"#,
            r#""usage":{"input_tokens":1234,"output_tokens":56,"#,
            r#""cached_input_tokens":1000,"total_tokens":1290}}"#
        ),
        "the contract's order, and the counters nobody reported left out"
    );
}

#[test]
fn a_field_nobody_reported_is_absent_and_never_null() {
    let report = Report {
        answer: "Nothing to say.".to_string(),
        turns: 1,
        stopped: Stopped::Final,
        model: None,
        tool_calls: 0,
        usage: Usage::default(),
        duration: Duration::from_millis(7),
    };

    let json = to_json(&report, SESSION);
    assert_eq!(
        json,
        concat!(
            r#"{"answer":"Nothing to say.","#,
            r#""session":"0f4a2f2e-6a3b-4b4d-9a2c-9d1f2a3b4c5d","#,
            r#""turns":1,"#,
            r#""stopped":"final","#,
            r#""duration_ms":7,"#,
            r#""tool_calls":0}"#
        ),
        "no model and no usage means no fields, not null fields"
    );
    assert!(!json.contains("null"), "never null: {json}");
}

#[test]
fn every_stop_reason_has_its_own_word() {
    for (stopped, word) in [
        (Stopped::Final, "final"),
        (Stopped::MaxTurns, "max_turns"),
        (Stopped::Cancelled, "cancelled"),
        (Stopped::Error, "error"),
    ] {
        let report = Report {
            answer: String::new(),
            turns: 1,
            stopped,
            model: None,
            tool_calls: 0,
            usage: Usage::default(),
            duration: Duration::ZERO,
        };
        let json = to_json(&report, SESSION);
        assert!(
            json.contains(&format!(r#""stopped":"{word}""#)),
            "`{stopped:?}` is written as `{word}`: {json}"
        );
    }
}

#[test]
fn an_answer_that_needs_escaping_stays_on_one_line() {
    let answer = "He said \"done\"\nthen\tleft.";
    let report = Report {
        answer: answer.to_string(),
        turns: 1,
        stopped: Stopped::Final,
        model: None,
        tool_calls: 0,
        usage: Usage::default(),
        duration: Duration::ZERO,
    };

    let json = to_json(&report, SESSION);
    assert!(
        !json.contains('\n'),
        "a newline in the answer must not become a second line: {json}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("the object parses back as it was written");
    assert_eq!(parsed["answer"], answer, "escaping loses nothing: {json}");
}
