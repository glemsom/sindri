//! Proves the three record files §13 puts in a session directory: `session.json`
//! says what the run was and how it ended, `transcript.jsonl` holds one fact per
//! line as the run goes, and `status.json` holds what the run said it was doing.
//!
//! The tests read the files with their own eyes rather than through the writer,
//! so the shape the file has is the shape asserted and a reader of a killed run
//! is what is proved. Every test holds its state home and its workspace in a temp
//! directory of its own and hands them in, so no real `XDG_STATE_HOME` or `HOME`
//! is read or written.

use std::io::{BufRead, BufReader};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::provider::Route;
use sindri_core::provider::Usage as Ask;
use sindri_core::records::{Entry, Facts, Record, Status, Told, read_transcript, timestamp};
use sindri_core::redact::{MASK, SecretSet};
use sindri_core::run::{Acted, Budget, Call, Exit, Limits, State, Stop};
use sindri_core::session::Session;
use sindri_core::wire::Wire;
use tempfile::TempDir;

/// A workspace, a state home and the run's facts, all of the test's own.
struct Ground {
    /// Held so the directories outlive the paths that name them.
    _root: TempDir,
    /// The one directory the run stands in.
    workspace: PathBuf,
    /// The state home the session directory is made under.
    state: PathBuf,
    /// The dials the run obeys.
    limits: Limits,
    /// Every literal the record must never write down.
    secrets: SecretSet,
}

impl Ground {
    /// A ground with an empty workspace and an empty state home.
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let workspace = root.path().join("workspace");
        let state = root.path().join("state");
        for path in [&workspace, &state] {
            std::fs::create_dir_all(path).expect("a directory");
        }
        Ground {
            _root: root,
            workspace,
            state,
            limits: Limits::default(),
            secrets: SecretSet::default(),
        }
    }

    /// This ground with `secret` among the literals the record must not carry.
    fn holding(mut self, secret: &str) -> Ground {
        self.secrets = SecretSet::of([secret]);
        self
    }

    /// A session directory of this ground's state home.
    fn session(&self) -> Session {
        Session::create(&self.state).expect("a session")
    }

    /// The boundary a run in this workspace holds.
    fn boundary(&self) -> Boundary {
        Boundary::from_config(&Config::default(), &self.workspace, Enforcement::Unsafe)
            .expect("the boundary holds")
    }

    /// The facts a session document opens with.
    fn facts<'a>(&'a self, boundary: &'a Boundary) -> Facts<'a> {
        Facts {
            wire: Wire::Anthropic,
            model: "claude-sonnet-4",
            route: None,
            boundary,
            limits: &self.limits,
            workspace: &self.workspace,
            secrets: &self.secrets,
        }
    }

    /// A record opened on `session` in this ground.
    fn record(&self, session: &Session) -> Record {
        let boundary = self.boundary();
        Record::open(session, self.facts(&boundary)).expect("a record")
    }
}

/// One of the records files, read as JSON.
fn json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("cannot read {}: {error}", path.display());
    });
    serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!("{} is not JSON: {error}\n{text}", path.display());
    })
}

#[test]
fn a_new_session_document_says_running_and_names_its_guid() {
    let ground = Ground::new();
    let session = ground.session();
    let record = ground.record(&session);
    drop(record);

    let document = json(&session.directory().join("session.json"));
    assert_eq!(
        document["session"],
        json!(session.guid()),
        "the GUID the directory is named by is the document's `session`"
    );
    assert_eq!(
        document["stopped"],
        json!("running"),
        "a run that has not stopped says so"
    );
    assert_eq!(document["turns"], json!(0), "no turn has started yet");
    assert!(
        document.get("end").is_none(),
        "no end is written before a stop reason lands: {document}"
    );

    assert_eq!(document["provider"], json!("anthropic"));
    assert_eq!(document["model"], json!("claude-sonnet-4"));
    assert_eq!(document["enforcement"], json!("unsafe"));
    assert_eq!(
        document["workspace"],
        json!(ground.workspace.display().to_string()),
        "the workspace is the one directory the run stands in"
    );
    assert_eq!(
        document["dials"]["max_turns"],
        json!(ground.limits.max_turns())
    );
    assert_eq!(
        document["policy"]["timeout_seconds"],
        json!(ground.boundary().timeout_seconds()),
        "the policy block is the boundary in force, read off the boundary"
    );
    assert!(
        document["policy"]["description"]
            .as_str()
            .is_some_and(|text| text.contains("Run a command with `bash`.")),
        "the policy handed to the model is the description the model was given: {document}"
    );
    assert!(
        document["start"]
            .as_str()
            .is_some_and(|text| text.len() == 20 && text.ends_with('Z')),
        "the start is an RFC 3339 instant in UTC: {document}"
    );
}

#[test]
fn a_session_document_says_which_route_answered_it() {
    let ground = Ground::new();
    let session = ground.session();
    let boundary = ground.boundary();
    let facts = Facts {
        wire: Wire::Copilot,
        model: "gpt-4o",
        route: Some(Route::Chat),
        boundary: &boundary,
        limits: &ground.limits,
        workspace: &ground.workspace,
        secrets: &ground.secrets,
    };
    let record = Record::open(&session, facts).expect("a record");
    drop(record);

    let document = json(&session.directory().join("session.json"));
    assert_eq!(
        document["route"],
        json!("chat"),
        "the endpoint the wire answered on is kept, so a mystery is debuggable: {document}"
    );
}

#[test]
fn a_document_of_a_wire_with_one_endpoint_names_no_route() {
    let ground = Ground::new();
    let session = ground.session();
    let record = ground.record(&session);
    drop(record);

    let document = json(&session.directory().join("session.json"));
    assert!(
        document.get("route").is_none(),
        "a field the run knows nothing about is left out, never null: {document}"
    );
}

#[test]
fn a_timestamp_is_rfc3339_in_utc_by_the_date_command() {
    // `date` is the independent statement of what an instant is called: not a
    // second copy of this code's own arithmetic.
    for seconds in [0u64, 1_000_000_000, 1_717_000_000, 4_102_444_800] {
        let instant = UNIX_EPOCH + Duration::from_secs(seconds);
        let truth = Command::new("date")
            .args(["-u", "-d", &format!("@{seconds}"), "+%Y-%m-%dT%H:%M:%SZ"])
            .output()
            .expect("date should run");
        let said = String::from_utf8(truth.stdout).expect("date prints UTF-8");
        assert_eq!(
            timestamp(instant),
            said.trim_end(),
            "{seconds} is not the instant date calls it"
        );
    }
    assert_eq!(
        timestamp(SystemTime::UNIX_EPOCH),
        "1970-01-01T00:00:00Z",
        "the epoch is the epoch"
    );
    // A clock that is wrong is still a clock: an instant before the epoch counts
    // backwards through a whole day and does not wrap.
    let before = UNIX_EPOCH - Duration::from_secs(1);
    let truth = Command::new("date")
        .args(["-u", "-d", "@-1", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .expect("date should run");
    assert_eq!(
        timestamp(before),
        String::from_utf8(truth.stdout)
            .expect("date prints UTF-8")
            .trim_end(),
        "an instant before the epoch is still the instant date calls it"
    );
}

/// Appends `line` to a records file, exactly as a newer `sindri` might write it.
fn push(path: &Path, line: &str) {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap_or_else(|error| panic!("cannot open {}: {error}", path.display()));
    writeln!(file, "{line}").unwrap_or_else(|error| panic!("cannot append: {error}"));
}

/// Every line of a records file, read as JSON.
fn lines(path: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("cannot read {}: {error}", path.display());
    });
    text.lines()
        .map(|line| serde_json::from_str(line).expect("every line is JSON"))
        .collect()
}

#[test]
fn a_turn_start_writes_one_entry_and_moves_the_turn_count() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);

    record.turn_started(1).expect("a turn start is recorded");
    record.turn_started(2).expect("a turn start is recorded");

    let written = lines(&session.directory().join("transcript.jsonl"));
    assert_eq!(
        written,
        vec![
            json!({"kind": "turn", "turn": 1}),
            json!({"kind": "turn", "turn": 2})
        ],
        "one entry per turn start, in the order they happened, as facts"
    );
    assert_eq!(
        json(&session.directory().join("session.json"))["turns"],
        json!(2),
        "the document counts the turns the transcript holds"
    );

    let transcript = read_transcript(session.directory()).expect("the transcript reads");
    assert_eq!(
        transcript.entries(),
        [Entry::Turn { turn: 1 }, Entry::Turn { turn: 2 }]
    );
    assert_eq!(transcript.skipped(), 0, "nothing was skipped");
}

#[test]
fn a_session_with_no_transcript_yet_reads_as_no_entries() {
    let ground = Ground::new();
    let session = ground.session();

    let transcript = read_transcript(session.directory()).expect("an empty transcript reads");
    assert!(transcript.entries().is_empty(), "nothing has happened yet");
    assert_eq!(transcript.skipped(), 0);
}

#[test]
fn a_tool_call_and_its_result_are_two_entries_of_their_own_facts() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);
    let call = Call::new("bash", r#"{"command": "ls"}"#);

    record.turn_started(1).expect("a turn start is recorded");
    record.called(1, &call).expect("a call is recorded");
    record
        .resulted(
            1,
            &call,
            &Acted {
                text: String::from("exit 0\na.txt\n"),
                bytes: 6,
                elapsed: Duration::from_millis(12),
                exit: Exit::Code(0),
            },
        )
        .expect("a result is recorded");

    assert_eq!(
        lines(&session.directory().join("transcript.jsonl")),
        vec![
            json!({"kind": "turn", "turn": 1}),
            json!({"kind": "call", "turn": 1, "name": "bash", "arguments": r#"{"command": "ls"}"#}),
            json!({
                "kind": "result",
                "turn": 1,
                "name": "bash",
                "text": "exit 0\na.txt\n",
                "bytes": 6,
                "elapsed_ms": 12,
                "exit": {"code": 0},
            }),
        ],
        "a call and what it did are facts, in the order they happened"
    );
}

/// How a call ended is a fact of its result and not a sentence a reader of the text
/// beside it works out (ADR-0007): the row that says `✗ exit 1` reads this field, so
/// §12's four states are four values the run wrote down rather than four ways of
/// wording the same prose.
#[test]
fn a_result_writes_the_verdict_its_call_settled_with() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);
    let call = Call::new("bash", r#"{"command": "ls"}"#);

    for (turn, exit) in [
        (1, Exit::Code(0)),
        (2, Exit::Code(1)),
        (3, Exit::TimedOut),
        (4, Exit::Cancelled),
    ] {
        record
            .resulted(
                turn,
                &call,
                &Acted {
                    text: String::from("exit 0\n"),
                    bytes: 7,
                    elapsed: Duration::from_millis(2),
                    exit,
                },
            )
            .expect("a result is recorded");
    }

    let written = std::fs::read_to_string(session.directory().join("transcript.jsonl"))
        .expect("the transcript");
    let verdicts: Vec<Value> = written
        .lines()
        .map(|line| {
            let entry: Value = serde_json::from_str(line).expect("every line is JSON");
            entry["exit"].clone()
        })
        .collect();
    assert_eq!(
        verdicts,
        vec![
            json!({"code": 0}),
            json!({"code": 1}),
            json!("timed_out"),
            json!("cancelled"),
        ],
        "four states are four values: {written}"
    );
    assert!(
        !written.contains("null"),
        "a field with nothing in it is left out rather than written as `null` (§15): {written}"
    );
}

/// A newer build may always write what an older one reads past (ADR-0007): a result
/// line written before the verdict existed still reads, and reads as a result with no
/// verdict rather than as a result that finished well.
#[test]
fn a_result_line_written_before_the_verdict_existed_still_reads() {
    let ground = Ground::new();
    let session = ground.session();
    let directory = session.directory();
    std::fs::write(
        directory.join("transcript.jsonl"),
        "{\"kind\": \"result\", \"turn\": 1, \"name\": \"bash\", \"text\": \"exit 0\\n\", \"bytes\": 7, \"elapsed_ms\": 2}\n",
    )
    .expect("an older transcript");

    let transcript = read_transcript(directory).expect("an older transcript reads");

    assert_eq!(transcript.skipped(), 0, "the line is an entry, not a skip");
    assert_eq!(
        transcript.entries(),
        [Entry::Result {
            turn: 1,
            name: SecretSet::default().redact("bash"),
            text: SecretSet::default().redact("exit 0\n"),
            bytes: 7,
            elapsed_ms: 2,
            exit: None,
        }],
        "a result nobody recorded a verdict for says nothing it does not know"
    );
}

#[test]
fn the_assistant_entry_lands_when_the_turn_ends() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);
    let call = Call::new("bash", r#"{"command": "ls"}"#);

    record.turn_started(1).expect("a turn start is recorded");
    record.called(1, &call).expect("a call is recorded");
    record
        .answered(1, "one file.")
        .expect("an answer is recorded");
    record.turn_started(2).expect("a turn start is recorded");
    record
        .answered(2, "nothing else.")
        .expect("an answer is recorded");

    let written = lines(&session.directory().join("transcript.jsonl"));
    assert_eq!(
        written.last(),
        Some(&json!({"kind": "assistant", "turn": 2, "text": "nothing else."})),
        "the turn ends with what the assistant said: {written:?}"
    );
    assert_eq!(
        written
            .iter()
            .filter(|entry| entry["kind"] == json!("assistant"))
            .count(),
        2,
        "one assistant entry per turn: {written:?}"
    );
}

/// A thought is a fact of its own: reasoning is written down beside the answer it
/// reached, under a `kind` of its own, so a reader can show a run's thinking without
/// the interface having to remember it — and a build that knows no `reasoning` skips
/// the line, which is how a newer `sindri` writes what an older one reads past.
#[test]
fn a_thought_lands_as_its_own_entry_beside_the_answer() {
    let ground = Ground::new().holding("hunter2");
    let session = ground.session();
    let mut record = ground.record(&session);

    record.turn_started(1).expect("a turn start is recorded");
    record
        .reasoned(1, "the key is hunter2, so read the file before answering")
        .expect("a thought is recorded");
    record
        .answered(1, "one file.")
        .expect("an answer is recorded");

    let written = lines(&session.directory().join("transcript.jsonl"));
    assert_eq!(
        written,
        vec![
            json!({"kind": "turn", "turn": 1}),
            json!({
                "kind": "reasoning",
                "turn": 1,
                "text": format!("the key is {MASK}, so read the file before answering"),
            }),
            json!({"kind": "assistant", "turn": 1, "text": "one file."}),
        ],
        "a thought is written when it lands, in the order the turn said it: {written:?}"
    );

    let transcript = read_transcript(session.directory()).expect("the transcript reads");
    assert_eq!(
        transcript.entries(),
        [
            Entry::Turn { turn: 1 },
            Entry::Reasoning {
                turn: 1,
                text: SecretSet::default().redact(format!(
                    "the key is {MASK}, so read the file before answering"
                )),
            },
            Entry::Assistant {
                turn: 1,
                text: SecretSet::default().redact("one file."),
            },
        ],
        "the thought reads back as its own fact, and the answer still follows it"
    );
    assert_eq!(transcript.skipped(), 0, "nothing was skipped");
}

#[test]
fn no_line_of_the_record_carries_a_secret() {
    let ground = Ground::new().holding("hunter2");
    let session = ground.session();
    let mut record = ground.record(&session);
    let call = Call::new("bash", r#"{"command": "echo hunter2"}"#);

    record.turn_started(1).expect("a turn start is recorded");
    record.called(1, &call).expect("a call is recorded");
    record
        .resulted(
            1,
            &call,
            &Acted {
                text: String::from("hunter2\n"),
                bytes: 8,
                elapsed: Duration::from_millis(1),
                exit: Exit::Code(0),
            },
        )
        .expect("a result is recorded");
    record
        .answered(1, "the key is hunter2")
        .expect("an answer is recorded");

    let path = session.directory().join("transcript.jsonl");
    let written = std::fs::read_to_string(&path).expect("the transcript");
    assert!(
        !written.contains("hunter2"),
        "the record is a window that cannot carry a secret: {written}"
    );
    assert!(
        written.matches(MASK).count() >= 3,
        "the arguments, the output and the answer are all masked: {written}"
    );
}

#[test]
fn a_stop_reason_lands_in_the_document_and_ends_the_run() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);
    let started = json(&session.directory().join("session.json"))["start"]
        .as_str()
        .expect("a start")
        .to_owned();

    record
        .stopped(&State::Stopped(Stop::Final))
        .expect("a stop reason is recorded");

    let document = json(&session.directory().join("session.json"));
    assert_eq!(document["stopped"], json!("final"));
    let ended = document["end"].as_str().expect("an end");
    assert!(
        ended >= started.as_str(),
        "the end does not come before the start: {started} .. {ended}"
    );
    assert_eq!(
        record.document().stopped.word(),
        "final",
        "the document in hand says what the file says"
    );
    assert_eq!(
        record.guid(),
        session.guid(),
        "the GUID the record hands out is the one the directory is named by, for `-v`"
    );
}

#[test]
fn every_stop_reason_writes_its_own_word_and_nothing_else() {
    let ground = Ground::new();
    let stops: [State; 5] = [
        State::Stopped(Stop::Final),
        State::Stopped(Stop::MaxTurns),
        State::Stopped(Stop::Budget(Budget::Clock)),
        State::Stopped(Stop::Cancelled),
        State::Stopped(Stop::Error(SecretSet::default().redact("the wire refused"))),
    ];

    for state in stops {
        let session = ground.session();
        let mut record = ground.record(&session);
        record.stopped(&state).expect("a stop reason is recorded");
        assert_eq!(
            json(&session.directory().join("session.json"))["stopped"],
            json!(state.word()),
            "the document says the word §4 says"
        );
    }
}

#[test]
fn a_running_run_that_is_asked_to_stop_is_not_given_an_end() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);

    record
        .stopped(&State::Running)
        .expect("a running state writes nothing new");

    let document = json(&session.directory().join("session.json"));
    assert_eq!(document["stopped"], json!("running"));
    assert!(document.get("end").is_none(), "{document}");
}

#[test]
fn the_usage_of_every_ask_is_summed_and_what_nobody_reported_is_left_out() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);

    for ask in [
        Ask {
            input: Some(10),
            output: Some(5),
            total: Some(15),
            cached_input: None,
            reasoning: None,
        },
        Ask {
            input: Some(1),
            output: Some(2),
            total: Some(3),
            cached_input: Some(7),
            reasoning: None,
        },
        Ask::default(),
    ] {
        record.charged(&ask).expect("an ask is charged");
    }

    assert_eq!(
        json(&session.directory().join("session.json"))["usage"],
        json!({"input": 11, "output": 7, "total": 18, "cached_input": 7}),
        "the run's usage is the sum of its asks, and a token nobody reported is absent"
    );
}

#[test]
fn a_status_document_is_one_line_of_the_runs_own_words() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);

    record
        .told(Told::Asking { turn: 3 })
        .expect("a status document is written");
    record
        .told(Told::Stopped {
            turn: 3,
            stop: Stop::Final,
        })
        .expect("a status document is written");

    let written = lines(&session.directory().join("status.json"));
    assert_eq!(
        written,
        vec![
            json!({
                "phase": "working",
                "message": "asking the model",
                "turn": 3,
                "workspace": ground.workspace.file_name().expect("a name").to_string_lossy(),
            }),
            json!({
                "phase": "done",
                "message": "answered",
                "turn": 3,
                "workspace": ground.workspace.file_name().expect("a name").to_string_lossy(),
            }),
        ],
        "the status documents are the run's own words, one per line, in order"
    );
}

#[test]
fn each_of_the_five_states_says_its_own_coarse_line() {
    let told = [
        (Told::Idle, "idle", "waiting for a prompt"),
        (Told::Asking { turn: 4 }, "working", "asking the model"),
        (
            Told::Waiting { turn: 4 },
            "blocked",
            "waiting for a command",
        ),
        (
            Told::Stopped {
                turn: 4,
                stop: Stop::Final,
            },
            "done",
            "answered",
        ),
        (
            Told::Stopped {
                turn: 4,
                stop: Stop::Error(SecretSet::default().redact("the wire said no")),
            },
            "error",
            "failed",
        ),
    ];

    for (told, phase, line) in told {
        let status = Status::new(told.clone(), Path::new("/work/sindri"));
        assert_eq!(status.phase().word(), phase, "the state of {told:?}");
        assert_eq!(status.message().to_string(), line, "the line of {told:?}");
        assert_eq!(status.workspace(), "sindri", "and the workspace's name");
    }
}

#[test]
fn a_status_line_never_carries_a_word_the_run_was_handed() {
    // The one place a run's own text could reach a status line is a stop reason
    // that carries it — a wire's refusal names the request back. The line is the
    // enum's own words and nothing else.
    let refused = "the model refused: temperature 9000 is outside [0, 1]";
    let told = Told::Stopped {
        turn: 2,
        stop: Stop::Error(SecretSet::default().redact(refused)),
    };

    let status = Status::new(told, Path::new("/work/sindri"));
    let written = String::from_utf8(status.json()).expect("a document is UTF-8");

    assert_eq!(status.message().to_string(), "failed");
    assert!(
        !written.contains("temperature"),
        "no word of the run's own reaches the status line: {written}"
    );
}

#[test]
fn every_turn_a_status_names_is_the_turn_it_was_told_at() {
    // `idle` is a run with no turn in it and says nothing rather than a turn zero, so a
    // reader is never told a turn the run has not had.
    assert_eq!(Told::Idle.turn(), 0);
    assert_eq!(Told::Asking { turn: 7 }.turn(), 7);
    assert_eq!(Told::Waiting { turn: 7 }.turn(), 7);
    assert_eq!(
        Told::Stopped {
            turn: 7,
            stop: Stop::Cancelled
        }
        .turn(),
        7
    );
}

#[test]
fn a_kind_this_build_does_not_know_is_skipped_and_the_rest_loads() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);
    let path = session.directory().join("transcript.jsonl");

    record.turn_started(1).expect("a turn start is recorded");
    // What a newer `sindri` may write: a kind this build has never heard of, its
    // own fields, and a line that was cut short by a kill.
    push(
        &path,
        r#"{"kind": "thinking", "turn": 1, "thought": "a newer build wrote this"}"#,
    );
    push(&path, r#"{"kind": "turn", "turn": "#);
    record.turn_started(2).expect("a turn start is recorded");

    let transcript = read_transcript(session.directory()).expect("the transcript reads");
    assert_eq!(
        transcript.entries(),
        [Entry::Turn { turn: 1 }, Entry::Turn { turn: 2 }],
        "an unknown kind and a torn line cost their own lines and nothing else"
    );
    assert_eq!(
        transcript.skipped(),
        2,
        "both were skipped, and neither panicked"
    );
}

#[test]
fn a_known_kind_ignores_fields_this_build_has_never_heard_of() {
    let ground = Ground::new();
    let session = ground.session();
    let path = session.directory().join("transcript.jsonl");

    push(
        &path,
        r#"{"kind": "turn", "turn": 7, "colour": "blue", "nested": {"a": [1, 2]}}"#,
    );

    let transcript = read_transcript(session.directory()).expect("the transcript reads");
    assert_eq!(
        transcript.entries(),
        [Entry::Turn { turn: 7 }],
        "a field this build does not know is ignored, not refused"
    );
    assert_eq!(transcript.skipped(), 0);
}

/// One git command in `directory`, with no home, no global config and no
/// identity of the machine's own to read or write.
fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("HOME", directory)
        .env("XDG_CONFIG_HOME", directory)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "sindri")
        .env("GIT_AUTHOR_EMAIL", "sindri@example.invalid")
        .env("GIT_COMMITTER_NAME", "sindri")
        .env("GIT_COMMITTER_EMAIL", "sindri@example.invalid")
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git prints UTF-8")
}

#[test]
fn finishing_empties_the_temp_and_keeps_the_record() {
    let ground = Ground::new();
    let session = ground.session();
    let mut record = ground.record(&session);
    record.turn_started(1).expect("a turn start is recorded");
    let scratch = session.temp().join("left-behind.txt");
    std::fs::write(&scratch, "scratch\n").expect("a file in the session temp");

    record.finish().expect("the session ends");

    assert!(
        !scratch.exists(),
        "the session temp is scratch for the life of the session and nothing after it"
    );
    assert!(
        session.temp().is_dir() && session.spill_dir().is_dir(),
        "the temp directory and spill are kept"
    );
    assert_eq!(
        json(&session.directory().join("session.json"))["turns"],
        json!(1),
        "the record is untouched by the end of the session"
    );
}

#[test]
fn the_document_names_the_branch_and_commit_the_workspace_stands_on() {
    let ground = Ground::new();
    git(&ground.workspace, &["init", "--quiet", "-b", "the-branch"]);
    std::fs::write(ground.workspace.join("a.txt"), "one file\n").expect("a file");
    git(&ground.workspace, &["add", "a.txt"]);
    git(
        &ground.workspace,
        &["commit", "--quiet", "-m", "one commit"],
    );
    let head = git(&ground.workspace, &["rev-parse", "HEAD"]);

    let session = ground.session();
    let record = ground.record(&session);
    drop(record);

    let document = json(&session.directory().join("session.json"));
    assert_eq!(document["branch"], json!("the-branch"));
    assert_eq!(
        document["commit"],
        json!(head.trim()),
        "the commit is the one the workspace stood on, whole"
    );
}

#[test]
fn a_workspace_that_is_no_checkout_says_so_rather_than_guessing() {
    let ground = Ground::new();
    let session = ground.session();
    let record = ground.record(&session);
    drop(record);

    let document = json(&session.directory().join("session.json"));
    assert!(
        document.get("branch").is_none() && document.get("commit").is_none(),
        "a field the run knows nothing about is left out, never null: {document}"
    );
}

/// The child half of `a_killed_run_still_reads`: it opens a session of its own,
/// writes one turn after another, and is killed by that test — so it never
/// returns anything but its own death.
#[test]
#[ignore = "spawned by a_killed_run_still_reads, which kills it"]
fn child_writes_entries_until_it_is_killed() {
    let Ok(state) = std::env::var("SINDRI_RECORDS_CHILD") else {
        // Run by hand with nothing to write into: there is no session to kill.
        return;
    };
    let state = PathBuf::from(state);
    let workspace = state.join("workspace");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    let session = Session::create(&state).expect("a session");
    let boundary = Boundary::from_config(&Config::default(), &workspace, Enforcement::Unsafe)
        .expect("the boundary holds");
    let limits = Limits::default();
    let secrets = SecretSet::default();
    let mut record = Record::open(
        &session,
        Facts {
            wire: Wire::Anthropic,
            model: "a-model-the-wire-serves",
            route: None,
            boundary: &boundary,
            limits: &limits,
            workspace: &workspace,
            secrets: &secrets,
        },
    )
    .expect("a record");
    let call = Call::new("bash", r#"{"command": "echo hello"}"#);
    let acted = Acted {
        text: String::from("exit 0\nhello\n"),
        bytes: 12,
        elapsed: Duration::from_millis(3),
        exit: Exit::Code(0),
    };

    println!("guid {}", session.guid());
    println!("dir {}", session.directory().display());
    for turn in 1..=1_000u32 {
        record.turn_started(turn).expect("a turn start is recorded");
        record.called(turn, &call).expect("a call is recorded");
        record
            .resulted(turn, &call, &acted)
            .expect("a result is recorded");
        record
            .answered(turn, "hello.")
            .expect("an answer is recorded");
        record
            .charged(&Ask {
                input: Some(11),
                output: Some(4),
                total: Some(15),
                cached_input: None,
                reasoning: None,
            })
            .expect("an ask is charged");
        println!("turn {turn}");
        // Slow enough that the parent's kill lands mid-run and not after it.
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_killed_run_still_reads() {
    let ground = Ground::new();
    let mut child = Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "--exact",
            "child_writes_entries_until_it_is_killed",
            "--ignored",
            "--nocapture",
        ])
        .env("SINDRI_RECORDS_CHILD", &ground.state)
        .env("XDG_STATE_HOME", &ground.state)
        .env("HOME", ground._root.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the child runs");

    // The child's own lines, marked, because the test harness prints lines of
    // its own on the same stream.
    let said = BufReader::new(child.stdout.take().expect("the child's stdout"));
    let (mut guid, mut directory, mut last) = (String::new(), PathBuf::new(), 0);
    for line in said.lines() {
        let line = line.expect("a line");
        let (marker, rest) = line.split_once(' ').unwrap_or(("", ""));
        match marker {
            "guid" => guid = rest.to_owned(),
            "dir" => directory = PathBuf::from(rest),
            "turn" => {
                last = rest.parse().expect("a turn number");
                if last == 5 {
                    break;
                }
            }
            _ => {}
        }
    }
    assert!(last >= 5, "the child wrote five turns before it was killed");
    child.kill().expect("the child is killed");
    let status = child.wait().expect("the child is reaped");
    assert_eq!(
        status.signal(),
        Some(9),
        "the child died of SIGKILL and of nothing else: {status:?}"
    );

    let transcript = read_transcript(&directory).expect("the killed run's transcript reads");
    assert_eq!(
        transcript.skipped(),
        0,
        "every line the killed run left is a whole entry"
    );
    let entries = transcript.entries();
    let expected = |turn: u32| {
        vec![
            Entry::Turn { turn },
            Entry::Call {
                turn,
                name: SecretSet::default().redact("bash"),
                arguments: SecretSet::default().redact(r#"{"command": "echo hello"}"#),
            },
            Entry::Result {
                turn,
                name: SecretSet::default().redact("bash"),
                text: SecretSet::default().redact("exit 0\nhello\n"),
                bytes: 12,
                elapsed_ms: 3,
                exit: Some(Exit::Code(0)),
            },
            Entry::Assistant {
                turn,
                text: SecretSet::default().redact("hello."),
            },
        ]
    };
    for turn in 1..=last {
        let base = (turn as usize - 1) * 4;
        assert!(
            entries.len() >= base + 4,
            "turn {turn} was written before the kill and is not whole in the transcript: {entries:?}"
        );
        assert_eq!(
            &entries[base..base + 4],
            expected(turn).as_slice(),
            "every entry up to the last turn boundary must be there"
        );
    }
    let in_flight = expected(last + 1);
    assert!(
        entries.len() - last as usize * 4 <= 4,
        "a kill costs the in-flight turn and nothing more: {entries:?}"
    );
    for (written, wanted) in entries[last as usize * 4..].iter().zip(&in_flight) {
        assert_eq!(
            written, wanted,
            "the in-flight turn is written in order, one entry at a time"
        );
    }

    let document = json(&directory.join("session.json"));
    assert_eq!(
        document["session"],
        json!(guid),
        "the GUID the child printed is the document's `session`"
    );
    assert_eq!(
        document["stopped"],
        json!("running"),
        "a killed run did not stop; it was left"
    );
    assert!(
        document["turns"].as_u64().is_some_and(|turns| turns >= 5),
        "the document counts the turns that started: {document}"
    );
    assert!(
        document["usage"]["input"]
            .as_u64()
            .is_some_and(|input| input >= 55),
        "the document's usage was rewritten as the run went too: {document}"
    );
    assert!(document.get("end").is_none(), "{document}");
}
