#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken list reports itself"
)]

//! The record as a list, and as markdown: `/sessions` and `/export` read what a
//! run left, from the same files `sindri sessions show` reads. Nothing here
//! writes a record: every fixture is a directory a run could have left, and the
//! listing is only ever a read of it.

use std::path::Path;

use sindri_core::paths::SessionDir;
use sindri_core::record::{Entry, Meta, Stored, summaries};
use tempfile::TempDir;

fn meta(id: &str, started_ms: u64, stopped: &str, turns: Option<u32>) -> Meta {
    Meta {
        id: id.to_string(),
        version: "0.1.0".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-mock".to_string(),
        enforcement: "bubblewrap".to_string(),
        pid: 4242,
        started_ms,
        ended_ms: (stopped != "running").then_some(started_ms + 1_000),
        duration_ms: (stopped != "running").then_some(1_000),
        answered_model: None,
        turns,
        tool_calls: turns.map(|turns| turns / 2),
        stopped: stopped.to_string(),
        usage: None,
        error: None,
    }
}

/// One record exactly as a run leaves it: `session.json` and one JSON object
/// per line of `transcript.jsonl`.
fn record(sessions: &Path, meta: Meta, entries: &[Entry]) -> SessionDir {
    let dir = SessionDir::at(sessions.join(&meta.id));
    std::fs::create_dir_all(dir.root()).expect("a session directory");
    std::fs::write(
        dir.root().join("session.json"),
        format!("{}\n", serde_json::to_string(&meta).expect("a document")),
    )
    .expect("the metadata");
    let mut transcript = String::new();
    for entry in entries {
        transcript.push_str(&serde_json::to_string(entry).expect("a line"));
        transcript.push('\n');
    }
    std::fs::write(dir.root().join("transcript.jsonl"), transcript).expect("the transcript");
    dir
}

fn user(text: &str) -> Entry {
    Entry::User {
        text: text.to_string(),
    }
}

fn answer(turn: u32, text: &str) -> Entry {
    Entry::Assistant {
        turn,
        text: text.to_string(),
        reasoning: None,
        usage: None,
        duration_ms: None,
    }
}

fn result(turn: u32, call_id: &str, content: &str) -> Entry {
    Entry::ToolResult {
        turn,
        call_id: call_id.to_string(),
        content: content.to_string(),
        is_error: false,
        outcome: None,
        exit_code: None,
        duration_ms: None,
        truncated: false,
        spill: Vec::new(),
    }
}

/// Three records a frame could have left, oldest first by `started_ms`.
fn three(sessions: &Path) {
    record(
        sessions,
        meta("aaaa", 1_000, "final", Some(3)),
        &[user("how many files?"), answer(3, "Twelve files.")],
    );
    record(
        sessions,
        meta("bbbb", 2_000, "max_turns", Some(7)),
        &[user("rewrite the parser"), answer(7, "Halfway.")],
    );
    record(
        sessions,
        meta("cccc", 3_000, "running", None),
        &[user("why is the gate red?")],
    );
}

#[test]
fn the_records_are_listed_newest_first_with_what_each_one_did() {
    let state = TempDir::new().expect("a state directory");
    let sessions = state.path().join("sessions");
    three(&sessions);
    // A directory that is not a record: a listing must not fall over on one.
    std::fs::create_dir_all(sessions.join("not-a-record")).expect("a stray directory");

    let listed = summaries(&sessions);
    let ids: Vec<&str> = listed.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["cccc", "bbbb", "aaaa"],
        "newest first, and a directory with no session.json is skipped"
    );

    let newest = &listed[0];
    assert_eq!(newest.started_ms, 3_000, "the time the row is sorted by");
    assert_eq!(newest.model, "claude-mock");
    assert_eq!(newest.stopped, "running");
    assert_eq!(newest.turns, None);
    assert_eq!(
        newest.prompt.as_deref(),
        Some("why is the gate red?"),
        "the first thing the human asked"
    );

    assert_eq!(listed[1].stopped, "max_turns");
    assert_eq!(listed[1].turns, Some(7));
    assert_eq!(listed[2].stopped, "final");
    assert_eq!(listed[2].prompt.as_deref(), Some("how many files?"));
}

#[test]
fn a_run_that_never_finished_says_running_and_still_reads_back() {
    let state = TempDir::new().expect("a state directory");
    let sessions = state.path().join("sessions");
    three(&sessions);

    let listed = summaries(&sessions);
    let unfinished = listed
        .iter()
        .find(|row| row.id == "cccc")
        .expect("the run that never finished is listed");
    assert_eq!(
        unfinished.stopped, "running",
        "the record says running, and the list says what the record says"
    );

    let stored = Stored::load_in(&SessionDir::at(sessions.join("cccc")))
        .expect("a record that never finished still reads");
    let rendered = stored.render();
    assert!(
        rendered.contains("why is the gate red?"),
        "the prompt is there: {rendered}"
    );
    assert!(
        rendered.contains("the run did not finish"),
        "and the reader is told it never ended: {rendered}"
    );
}

#[test]
fn the_markdown_export_says_every_text_the_record_says() {
    let state = TempDir::new().expect("a state directory");
    let sessions = state.path().join("sessions");
    let dir = record(
        &sessions,
        meta("dddd", 1_000, "final", Some(2)),
        &[
            user("how many files?"),
            Entry::Assistant {
                turn: 1,
                text: "Looking.".to_string(),
                reasoning: Some("counting".to_string()),
                usage: None,
                duration_ms: None,
            },
            Entry::ToolCall {
                turn: 1,
                call_id: "call_1".to_string(),
                tool: "bash".to_string(),
                command: Some("ls | wc -l".to_string()),
                problem: None,
            },
            result(1, "call_1", "12\n"),
            answer(2, "Twelve files."),
        ],
    );
    let stored = Stored::load_in(&dir).expect("the record reads");

    let markdown = stored.markdown();
    assert!(
        markdown.starts_with("# session dddd"),
        "the export names the record it came from: {markdown}"
    );
    // What `sindri sessions show` prints and what the export writes are the
    // same conversation: every entry's own text is in both.
    let shown = stored.render();
    for text in [
        "how many files?",
        "Looking.",
        "ls | wc -l",
        "12",
        "Twelve files.",
    ] {
        assert!(
            markdown.contains(text),
            "the export says {text:?}: {markdown}"
        );
        assert!(shown.contains(text), "and so does the record: {shown}");
    }
    assert!(
        markdown.contains("```") && markdown.contains("## "),
        "it is markdown, headings and fences: {markdown}"
    );
}

#[test]
fn a_tool_result_that_quotes_a_fence_does_not_close_the_export() {
    let state = TempDir::new().expect("a state directory");
    let sessions = state.path().join("sessions");
    let dir = record(
        &sessions,
        meta("eeee", 1_000, "final", Some(1)),
        &[
            user("show me the file"),
            result(1, "call_1", "```\nnot the end\n```\n"),
        ],
    );
    let markdown = Stored::load_in(&dir).expect("the record reads").markdown();
    assert!(
        markdown.contains("````\n```\nnot the end\n```\n````"),
        "the block is fenced wider than what it holds: {markdown}"
    );
}
