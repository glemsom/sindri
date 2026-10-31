#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken record reports itself"
)]

//! What a turn and a call cost, kept as fields beside the text the model was
//! shown: per-turn usage and duration on an `Assistant` entry, and a call's
//! outcome, exit code, duration, truncation and spill paths on a `ToolResult`.
//! `content` never changes — the fields are a second copy, not a replacement —
//! and a record written before they existed still reads.

mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sindri_core::agent::BashTool;
use sindri_core::exec::{Outcome, ToolResult};
use sindri_core::paths::SessionDir;
use sindri_core::provider::{ToolOutcome, Usage};
use sindri_core::record::{Cost, Entry, Recorder, Redaction, Start, Stored};
use sindri_core::sandbox::Enforcement;
use support::{Harness, Interrupt, Scripted, answer, call, calls, exited, fixtures};
use tempfile::TempDir;

fn recording() -> (TempDir, SessionDir, Recorder) {
    let root = TempDir::new().expect("state dir");
    let session = SessionDir::at(root.path().join("sessions/session-1"));
    let recorder = Recorder::start(
        &session,
        Start {
            provider: "anthropic".to_string(),
            model: "claude-mock".to_string(),
            enforcement: Enforcement::Bubblewrap,
        },
        Redaction::default(),
    )
    .expect("the record starts");
    (root, session, recorder)
}

fn usage(input: u64, output: u64) -> Usage {
    Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        cached_input_tokens: None,
        reasoning_tokens: None,
        total_tokens: None,
    }
}

/// A tool result of every shape the tool produces: a command that exited with a
/// status, one that ran out of time, and a sandbox that never started.
fn ran(result: ToolResult) -> ToolOutcome {
    let (_workspace, _session_temp, policy) = fixtures();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let tool = BashTool::new(&policy, Arc::new(Scripted::new(vec![result], seen)));
    tool.call(&call("call_1", "ls | wc -l"))
        .expect("the run was not cancelled")
}

#[test]
fn a_tool_result_entry_carries_the_facts_its_content_states() {
    let (_root, session, recorder) = recording();
    let (_workspace, session_temp, _policy) = fixtures();
    let spill = session_temp.join("spill/bash-1.out");
    std::fs::create_dir_all(spill.parent().expect("spill dir")).expect("spill dir");
    std::fs::write(&spill, "the whole story").expect("spill");

    let outcome = ran(ToolResult {
        outcome: Outcome::Exited { exit_code: 7 },
        stdout: "twelve files".to_string(),
        stderr: "a warning".to_string(),
        truncated: true,
        spill: vec![spill.clone()],
        duration_ms: 42,
    });
    let asked = call("call_1", "ls | wc -l");
    recorder.tool_call(1, &asked);
    recorder.tool_result(1, &outcome);

    let stored = Stored::load_in(&session).expect("the record reads back");
    let Some(Entry::ToolResult {
        outcome: word,
        exit_code,
        duration_ms,
        truncated,
        spill: paths,
        content,
        ..
    }) = stored.entries.last()
    else {
        panic!(
            "the result is the last thing on the record: {:?}",
            stored.entries
        );
    };
    assert_eq!(word.as_deref(), Some("exited"), "the outcome as a word");
    assert_eq!(*exit_code, Some(7));
    assert_eq!(*duration_ms, Some(42));
    assert!(*truncated, "the output was cut");
    assert_eq!(
        paths,
        &vec![spill.display().to_string()],
        "the path the model was told, not one made up at read time"
    );
    assert_eq!(
        content,
        &format!(
            "outcome: exited\nexit_code: 7\nduration_ms: 42\ntruncated: true\nspill: {}\n\
             stdout:\ntwelve files\nstderr:\na warning\n",
            spill.display()
        ),
        "the text the model reads is byte for byte what it read before the fields existed"
    );
}

#[test]
fn a_call_that_timed_out_and_one_whose_sandbox_never_started_say_so() {
    let (_root, session, recorder) = recording();
    let timed_out = ran(ToolResult {
        outcome: Outcome::TimedOut,
        stdout: String::new(),
        stderr: String::new(),
        truncated: false,
        spill: Vec::new(),
        duration_ms: 30_000,
    });
    let never_started = ran(ToolResult {
        outcome: Outcome::SandboxError {
            detail: "setting up uid map: Permission denied".to_string(),
        },
        stdout: String::new(),
        stderr: String::new(),
        truncated: false,
        spill: Vec::new(),
        duration_ms: 2,
    });
    recorder.tool_result(1, &timed_out);
    recorder.tool_result(2, &never_started);

    let stored = Stored::load_in(&session).expect("the record reads back");
    let facts: Vec<(&str, Option<i32>, Option<u64>, &str)> = stored
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::ToolResult {
                outcome,
                exit_code,
                duration_ms,
                content,
                ..
            } => Some((
                outcome.as_deref().unwrap_or("none"),
                *exit_code,
                *duration_ms,
                content.as_str(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        facts,
        vec![
            (
                "timed_out",
                None,
                Some(30_000),
                "outcome: timed_out\nduration_ms: 30000\ntruncated: false\nspill: none\n\
                 stdout:\nstderr:\n",
            ),
            (
                "sandbox_error",
                None,
                Some(2),
                "outcome: sandbox_error\ndetail: setting up uid map: Permission denied\n\
                 duration_ms: 2\ntruncated: false\nspill: none\nstdout:\nstderr:\n",
            ),
        ],
        "an outcome word is never a status: only an exit has an exit code, and \
         the text the model reads is what it read before the fields existed"
    );
}

#[test]
fn an_assistant_turn_carries_its_own_usage_and_a_duration() {
    let (_root, session, recorder) = recording();
    recorder.assistant(
        1,
        "Looking.",
        Some("counting"),
        Cost::measured(usage(11, 7), Duration::from_millis(812)),
    );
    recorder.assistant(2, "Twelve files.", None, Cost::default());

    let stored = Stored::load_in(&session).expect("the record reads back");
    assert_eq!(
        turn_costs(&stored),
        vec![(1, Some(usage(11, 7)), Some(812)), (2, None, None)],
        "a turn nobody measured says so rather than claiming zero"
    );
}

#[test]
fn a_batch_run_records_one_assistant_entry_per_turn_with_what_it_cost() {
    let mut h = Harness::new(
        vec![
            calls(vec![call("call_1", "ls | wc -l")]),
            answer("Twelve files."),
        ],
        vec![exited("twelve\n")],
        8,
        false,
    );
    let (_root, session, recorder) = attach(&mut h);
    let report = h.run("how many files?");
    recorder.finish(&report).expect("the record ends");

    let stored = Stored::load_in(&session).expect("the record reads back");
    let turns = turn_costs(&stored);
    assert_eq!(turns.len(), 2, "one entry per turn: {turns:?}");
    assert_eq!(turns[0].0, 1);
    assert_eq!(turns[1].0, 2);
    assert_eq!(
        turns[0].1,
        Some(usage(20, 5)),
        "the turn's own counters, not the run's total"
    );
    assert_eq!(turns[1].1, Some(usage(11, 7)));
    assert!(
        turns.iter().all(|(_, _, duration)| duration.is_some()),
        "every turn that ran is clocked: {turns:?}"
    );
    assert_eq!(
        stored.meta.usage.as_ref().and_then(|u| u.input_tokens),
        Some(31),
        "the run's total stays its own, not a sum the reader makes"
    );
}

#[test]
fn a_command_cancelled_mid_flight_leaves_a_call_with_no_result() {
    let mut h = Harness::interrupted(
        vec![
            calls(vec![call("call_1", "sleep 30")]),
            answer("never asked"),
        ],
        vec![exited("")],
        Interrupt::DuringFirstCommand,
    );
    let (_root, session, recorder) = attach(&mut h);
    let report = h.run("probe the tree");
    recorder.finish(&report).expect("the record ends");

    let stored = Stored::load_in(&session).expect("a cancelled run's record reads back");
    let calls: Vec<&str> = stored
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::ToolCall { call_id, .. } => Some(call_id.as_str()),
            Entry::ToolResult { call_id, .. } => Some(call_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        vec!["call_1"],
        "the call started and no result is invented for it"
    );
    assert!(
        stored.render().contains("sleep 30"),
        "and the reader still sees what was running: {}",
        stored.render()
    );
}

#[test]
fn a_record_written_before_the_new_fields_still_reads_and_renders_unchanged() {
    let (_root, session, _recorder) = recording();
    std::fs::write(
        session.root().join("session.json"),
        r#"{"id":"0a1b","version":"test","provider":"anthropic","model":"claude-mock","enforcement":"bubblewrap","pid":1,"started_ms":1700000000000,"stopped":"running"}
"#,
    )
    .expect("the metadata");
    std::fs::write(
        session.root().join("transcript.jsonl"),
        "{\"type\":\"user\",\"text\":\"how many files?\"}\n\
         {\"type\":\"assistant\",\"turn\":1,\"text\":\"Looking.\",\"reasoning\":\"counting\"}\n\
         {\"type\":\"tool_call\",\"turn\":1,\"call_id\":\"call_1\",\"tool\":\"bash\",\"command\":\"ls | wc -l\"}\n\
         {\"type\":\"tool_result\",\"turn\":1,\"call_id\":\"call_1\",\"content\":\"outcome: exited\\nexit_code: 0\\n\",\"is_error\":false}\n",
    )
    .expect("the transcript");

    let stored = Stored::load_in(&session).expect("a record from before the fields still reads");
    assert_eq!(stored.unread, 0, "nothing in it is unreadable");
    assert_eq!(stored.entries.len(), 4);
    assert!(
        matches!(
            &stored.entries[1],
            Entry::Assistant {
                usage: None,
                duration_ms: None,
                ..
            }
        ),
        "a turn written without a cost has none: {:?}",
        stored.entries
    );
    assert!(
        matches!(
            &stored.entries[3],
            Entry::ToolResult {
                outcome: None,
                exit_code: None,
                duration_ms: None,
                truncated: false,
                spill,
                ..
            } if spill.is_empty()
        ),
        "and a result written without facts has none: {:?}",
        stored.entries
    );
    assert_eq!(
        stored.render(),
        BEFORE_THE_FIELDS,
        "the readable transcript of an old record is byte for byte what it was"
    );
}

const BEFORE_THE_FIELDS: &str = "\
session 0a1b (anthropic/claude-mock); recorded by sindri test
sandbox: bubblewrap
started: 1700000000000 ms since the epoch
ended: never — the run did not finish
stopped: running
usage: none reported

--- user ---
how many files?

--- assistant (turn 1) ---
Looking.
reasoning: counting

--- bash (turn 1) call_1 ---
$ ls | wc -l

--- result (turn 1) call_1 ---
  outcome: exited
  exit_code: 0
";

/// Every assistant turn and what it cost, in the order the record holds them.
fn turn_costs(stored: &Stored) -> Vec<(u32, Option<Usage>, Option<u64>)> {
    stored
        .entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Assistant {
                turn,
                usage,
                duration_ms,
                ..
            } => Some((*turn, usage.clone(), *duration_ms)),
            _ => None,
        })
        .collect()
}

/// Attaches a recorder to a harness's agent: the run writes the record, and the
/// test reads it back from the directory the way `sessions show` does.
fn attach(h: &mut Harness) -> (TempDir, SessionDir, Arc<Recorder>) {
    let root = TempDir::new().expect("state dir");
    let session = SessionDir::at(root.path().join("sessions/session-1"));
    let recorder = Arc::new(
        Recorder::start(
            &session,
            Start {
                provider: "anthropic".to_string(),
                model: "claude-mock".to_string(),
                enforcement: Enforcement::Bubblewrap,
            },
            Redaction::default(),
        )
        .expect("the record starts"),
    );
    h.set_recorder(Arc::clone(&recorder));
    (root, session, recorder)
}
