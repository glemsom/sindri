#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

//! The turn loop's contract, proven at the two seams the spec names: a scripted
//! model and the bash-executor port. No bubblewrap, no network: what is under
//! test here is the loop itself — when it stops, what it feeds back, and what it
//! reports.

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use sindri_core::agent::{Agent, Event, Stopped};
use sindri_core::exec::{BashExecutor, Outcome, ToolResult};
use sindri_core::provider::{Call, Error, Message, Model, ToolSpec, Turn, Usage};
use sindri_core::sandbox::{Enforcement, NetworkPosture, SandboxPolicy, WritableSet};
use tempfile::TempDir;

/// A model that answers from a script and records every conversation it was
/// shown, so a test can prove what the loop fed back.
struct Scripted {
    replies: Mutex<Vec<Turn>>,
    seen: Arc<Mutex<Vec<Vec<Message>>>>,
    thinking: Arc<Mutex<Vec<bool>>>,
}

impl Scripted {
    fn new(replies: Vec<Turn>, seen: Arc<Mutex<Vec<Vec<Message>>>>) -> Self {
        Scripted {
            replies: Mutex::new(replies),
            seen,
            thinking: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Model for Scripted {
    fn turn(
        &self,
        history: &[Message],
        _tools: &[ToolSpec],
        thinking: bool,
    ) -> Result<Turn, Error> {
        self.seen.lock().expect("seen").push(history.to_vec());
        self.thinking.lock().expect("thinking").push(thinking);
        let mut replies = self.replies.lock().expect("replies");
        if replies.is_empty() {
            panic!("the loop asked for more turns than the script holds");
        }
        Ok(replies.remove(0))
    }
}

/// An executor that answers from a script and records the scripts it ran.
struct Commands {
    replies: Mutex<Vec<ToolResult>>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl BashExecutor for Commands {
    fn execute(&self, script: &str) -> ToolResult {
        self.seen.lock().expect("seen").push(script.to_string());
        let mut replies = self.replies.lock().expect("replies");
        if replies.is_empty() {
            return exited("");
        }
        replies.remove(0)
    }
}

fn exited(stdout: &str) -> ToolResult {
    ToolResult {
        outcome: Outcome::Exited { exit_code: 0 },
        stdout: stdout.to_string(),
        stderr: String::new(),
        truncated: false,
        spill: Vec::new(),
        duration_ms: 1,
    }
}

fn answer(text: &str) -> Turn {
    Turn {
        text: text.to_string(),
        reasoning: None,
        calls: Vec::new(),
        model: Some("claude-mock".to_string()),
        usage: usage(11, 7),
    }
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

/// A turn that asks for one tool call and says nothing else.
fn calls(calls: Vec<Call>) -> Turn {
    Turn {
        text: String::new(),
        reasoning: None,
        calls,
        model: Some("claude-mock".to_string()),
        usage: usage(20, 5),
    }
}

fn policy(workspace: &TempDir, session_temp: &Path) -> SandboxPolicy {
    let writable = WritableSet::new([workspace.path().to_path_buf()]).expect("writable set");
    SandboxPolicy::new(
        writable,
        session_temp.to_path_buf(),
        "/usr/bin:/bin".to_string(),
        NetworkPosture::Shared,
        Enforcement::Bubblewrap,
    )
    .expect("policy")
}

/// The loop under test, with a model that answers `replies`, an executor that
/// runs `commands`, and the channel its events land on. The workspace lives as
/// long as the harness, because the policy points at it.
struct Harness {
    agent: Agent,
    seen: Arc<Mutex<Vec<Vec<Message>>>>,
    ran: Arc<Mutex<Vec<String>>>,
    events: Receiver<Event>,
    thinking: Arc<Mutex<Vec<bool>>>,
    _workspace: TempDir,
}

impl Harness {
    fn new(replies: Vec<Turn>, commands: Vec<ToolResult>, max_turns: u32, thinking: bool) -> Self {
        let workspace = TempDir::new().expect("workspace");
        let session_temp = workspace.path().join("session/tmp");
        std::fs::create_dir_all(session_temp.join("home")).expect("session temp");
        let policy = policy(&workspace, &session_temp);

        let seen = Arc::new(Mutex::new(Vec::new()));
        let ran = Arc::new(Mutex::new(Vec::new()));
        let executor = Arc::new(Commands {
            replies: Mutex::new(commands),
            seen: Arc::clone(&ran),
        });
        let tool = sindri_core::agent::BashTool::new(&policy, executor);
        let model = Scripted::new(replies, Arc::clone(&seen));
        let told = Arc::clone(&model.thinking);
        let (sender, events) = std::sync::mpsc::channel();

        let mut agent = Agent::new(Box::new(model), tool, max_turns, thinking);
        agent.set_events(sender);
        Harness {
            agent,
            seen,
            ran,
            events,
            thinking: told,
            _workspace: workspace,
        }
    }

    fn run(&mut self, prompt: &str) -> sindri_core::agent::Report {
        self.agent.run(prompt).expect("the loop completed")
    }

    fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
}

#[test]
fn a_turn_that_asks_for_nothing_is_the_answer() {
    let mut h = Harness::new(vec![answer("Two files changed.")], Vec::new(), 8, false);

    let report = h.run("summarise this");

    assert_eq!(report.answer, "Two files changed.");
    assert_eq!(report.turns, 1, "one round trip");
    assert_eq!(report.stopped, Stopped::Final, "the model finished");
    assert_eq!(report.tool_calls, 0);
    assert_eq!(report.model.as_deref(), Some("claude-mock"));
    assert_eq!(report.usage.input_tokens, Some(11));
    assert_eq!(report.usage.output_tokens, Some(7));
    assert!(
        h.ran.lock().expect("ran").is_empty(),
        "nothing was executed"
    );
    assert!(h.events().is_empty(), "silence emits nothing");

    let seen = h.seen.lock().expect("seen");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0], vec![Message::User("summarise this".to_string())]);
}

#[test]
fn one_tool_call_runs_and_its_result_is_fed_back() {
    let mut h = Harness::new(
        vec![
            calls(vec![call("call_1", "ls | wc -l")]),
            answer("Twelve files."),
        ],
        vec![exited("twelve\n")],
        8,
        false,
    );

    let report = h.run("how many files?");

    assert_eq!(report.answer, "Twelve files.");
    assert_eq!(report.turns, 2, "one turn to ask, one to answer");
    assert_eq!(report.stopped, Stopped::Final);
    assert_eq!(report.tool_calls, 1);
    assert_eq!(*h.ran.lock().expect("ran"), vec!["ls | wc -l"]);
    assert_eq!(
        report.usage,
        usage(31, 12),
        "usage is accumulated across turns"
    );

    let seen = h.seen.lock().expect("seen");
    assert_eq!(seen.len(), 2, "the model saw two conversations");
    assert_eq!(seen[1].len(), 3, "user, assistant, tool results");
    assert_eq!(
        seen[1][1],
        Message::Assistant {
            text: String::new(),
            calls: vec![call("call_1", "ls | wc -l")],
        },
        "the model's own call is replayed"
    );
    match &seen[1][2] {
        Message::ToolResults(outcomes) => {
            assert_eq!(outcomes.len(), 1);
            assert_eq!(outcomes[0].call_id, "call_1");
            assert!(!outcomes[0].is_error);
            assert!(
                outcomes[0].content.contains("outcome: exited")
                    && outcomes[0].content.contains("twelve"),
                "the result reaches the model: {}",
                outcomes[0].content
            );
        }
        other => panic!("expected tool results, got {other:?}"),
    }

    let events: Vec<Event> = h.events();
    assert_eq!(
        events,
        vec![Event::ToolCall {
            turn: 1,
            id: "call_1".to_string(),
            command: Some("ls | wc -l".to_string()),
        }],
        "one event per call, named before it runs"
    );
}

#[test]
fn several_tool_calls_in_one_turn_all_run_and_come_back_together() {
    let mut h = Harness::new(
        vec![
            Turn {
                reasoning: Some("I need to look at two things.".to_string()),
                ..calls(vec![call("call_a", "echo one"), call("call_b", "echo two")])
            },
            answer("Both done."),
        ],
        vec![exited("one\n"), exited("two\n")],
        8,
        false,
    );

    let report = h.run("do two things");

    assert_eq!(report.tool_calls, 2);
    assert_eq!(report.turns, 2, "both calls are answered in one turn");
    assert_eq!(*h.ran.lock().expect("ran"), vec!["echo one", "echo two"]);

    let seen = h.seen.lock().expect("seen");
    match &seen[1][2] {
        Message::ToolResults(outcomes) => {
            assert_eq!(outcomes.len(), 2, "both results come back");
            assert_eq!(outcomes[0].call_id, "call_a");
            assert_eq!(outcomes[1].call_id, "call_b");
            assert!(outcomes[0].content.contains("one"));
            assert!(outcomes[1].content.contains("two"));
        }
        other => panic!("expected tool results, got {other:?}"),
    }

    let events: Vec<Event> = h.events();
    assert_eq!(
        events,
        vec![
            Event::Reasoning {
                turn: 1,
                text: "I need to look at two things.".to_string(),
            },
            Event::ToolCall {
                turn: 1,
                id: "call_a".to_string(),
                command: Some("echo one".to_string()),
            },
            Event::ToolCall {
                turn: 1,
                id: "call_b".to_string(),
                command: Some("echo two".to_string()),
            },
        ]
    );
}

#[test]
fn a_call_with_malformed_arguments_is_answered_not_run() {
    let broken = Call::new(
        "call_bad",
        "bash",
        Err("the arguments were not a JSON object: `{oops`".to_string()),
    );
    let mut h = Harness::new(
        vec![calls(vec![broken]), answer("Retried with valid arguments.")],
        Vec::new(),
        8,
        false,
    );

    let report = h.run("do something");

    assert_eq!(report.answer, "Retried with valid arguments.");
    assert!(
        h.ran.lock().expect("ran").is_empty(),
        "a malformed call never reaches the sandbox"
    );
    assert_eq!(report.tool_calls, 1, "it is answered, so it is counted");

    let seen = h.seen.lock().expect("seen");
    match &seen[1][2] {
        Message::ToolResults(outcomes) => {
            assert!(outcomes[0].is_error, "the tool says it did not run");
            assert!(
                outcomes[0].content.contains("not run")
                    && outcomes[0].content.contains("not a JSON object"),
                "the model is told why: {}",
                outcomes[0].content
            );
        }
        other => panic!("expected tool results, got {other:?}"),
    }

    let events: Vec<Event> = h.events();
    assert_eq!(
        events,
        vec![Event::ToolCall {
            turn: 1,
            id: "call_bad".to_string(),
            command: None,
        }],
        "a call that cannot run is still announced"
    );
}

#[test]
fn the_turn_cap_ends_the_run_with_whatever_the_last_turn_said() {
    let mut h = Harness::new(
        vec![
            calls(vec![call("call_1", "true")]),
            calls(vec![call("call_2", "true")]),
        ],
        vec![exited(""), exited("")],
        2,
        false,
    );

    let report = h.run("loop forever");

    assert_eq!(report.turns, 2, "the cap is exactly two round trips");
    assert_eq!(report.stopped, Stopped::MaxTurns);
    assert_eq!(report.answer, "", "the partial answer is whatever exists");
    assert_eq!(
        h.seen.lock().expect("seen").len(),
        2,
        "no turn beyond the cap"
    );
}

#[test]
fn reasoning_is_off_unless_the_run_asks_for_it() {
    let mut quiet = Harness::new(vec![answer("done")], Vec::new(), 8, false);
    quiet.run("why is the sky blue");
    assert_eq!(
        *quiet.thinking.lock().expect("log"),
        vec![false],
        "reasoning is off by default"
    );

    let mut paying = Harness::new(vec![answer("done")], Vec::new(), 8, true);
    paying.run("why is the sky blue");
    assert_eq!(
        *paying.thinking.lock().expect("log"),
        vec![true],
        "the flag turns reasoning on"
    );
}

#[test]
fn reasoning_text_reaches_the_event_stream_when_present() {
    let mut h = Harness::new(
        vec![Turn {
            reasoning: Some("The diff shows one renamed symbol.".to_string()),
            ..answer("A rename.")
        }],
        Vec::new(),
        8,
        false,
    );

    let report = h.run("what changed");

    assert_eq!(report.answer, "A rename.");
    let events: Vec<Event> = h.events();
    assert_eq!(
        events,
        vec![Event::Reasoning {
            turn: 1,
            text: "The diff shows one renamed symbol.".to_string(),
        }]
    );
}

#[test]
fn usage_accumulates_across_turns_cached_and_reasoning_tokens_included() {
    let first = Turn {
        usage: Usage {
            input_tokens: Some(120),
            output_tokens: Some(40),
            cached_input_tokens: Some(100),
            reasoning_tokens: Some(30),
            total_tokens: Some(160),
        },
        ..calls(vec![call("call_1", "true")])
    };
    let second = Turn {
        usage: Usage {
            input_tokens: Some(80),
            output_tokens: Some(10),
            cached_input_tokens: None,
            reasoning_tokens: Some(5),
            total_tokens: Some(90),
        },
        ..answer("Done.")
    };
    let mut h = Harness::new(vec![first, second], vec![exited("")], 8, true);

    let report = h.run("do it");

    assert_eq!(
        report.usage,
        Usage {
            input_tokens: Some(200),
            output_tokens: Some(50),
            cached_input_tokens: Some(100),
            reasoning_tokens: Some(35),
            total_tokens: Some(250),
        },
        "every counter is summed, and an unreported one stays unreported"
    );
}

#[test]
fn a_counter_nobody_reported_stays_absent_rather_than_becoming_zero() {
    let mut h = Harness::new(vec![answer("Done.")], Vec::new(), 8, false);

    let report = h.run("do it");

    assert_eq!(report.usage.cached_input_tokens, None);
    assert_eq!(report.usage.reasoning_tokens, None);
    assert_eq!(report.usage.total_tokens, None);
}
