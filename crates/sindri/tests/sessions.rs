//! What `sindri sessions` hands a person and a script: `list` newest first, `show` from a whole id or an
//! unambiguous prefix, and `--json` in the files' own shape, read back as bytes from the spawned binary.
//! Every session is a temp `XDG_STATE_HOME` written by hand, so no test needs a network, a key or a config.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

struct State {
    /// Held so the directory outlives the paths that name it.
    _root: TempDir,
    home: PathBuf,
}

impl State {
    fn empty() -> State {
        let root = tempfile::tempdir().expect("a temp state home");
        let home = root.path().join("state");
        std::fs::create_dir_all(&home).expect("a state home");
        State { _root: root, home }
    }

    fn sessions(&self) -> PathBuf {
        self.home.join("sindri").join("sessions")
    }

    fn session(&self, guid: &str, wire: &str, model: &str, start: &str, stopped: &str) -> PathBuf {
        let directory = self.sessions().join(guid);
        std::fs::create_dir_all(directory.join("tmp")).expect("a session temp");
        std::fs::create_dir_all(directory.join("spill")).expect("a spill directory");
        let document = json!({
            "session": guid,
            "provider": wire,
            "model": model,
            "enforcement": "bubblewrap",
            "policy": {
                "enforcement": "bubblewrap",
                "writable": ["/workspace"],
                "timeout_seconds": 120,
                "command_budget_ms": 3600000,
                "output_budget_bytes": 8388608,
                "description": "the boundary the model was told about",
            },
            "workspace": "/workspace",
            "branch": "main",
            "commit": "1a2b3c4d5e6f",
            "dials": {"max_turns": 24, "command_budget_ms": 3600000, "output_budget_bytes": 8388608},
            "start": start,
            "turns": 2,
            "usage": {"input": 11, "output": 7},
            "stopped": stopped,
        });
        std::fs::write(directory.join("session.json"), format!("{document:#}\n"))
            .expect("a session document");
        directory
    }

    fn transcript(&self, session: &Path, entries: &[Value]) {
        let text: String = entries.iter().map(|entry| format!("{entry}\n")).collect();
        std::fs::write(session.join("transcript.jsonl"), text).expect("a transcript");
    }

    fn statuses(&self, session: &Path, documents: &[Value]) {
        let text: String = documents
            .iter()
            .map(|document| format!("{document}\n"))
            .collect();
        std::fs::write(session.join("status.json"), text).expect("a status file");
    }

    fn sindri(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sindri"))
            .env_clear()
            .env("XDG_STATE_HOME", &self.home)
            .args(args)
            .output()
            .expect("the binary runs")
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn sessions_list_names_every_session_newest_first() {
    let state = State::empty();
    // Named so the directory's order, the GUID's order and the start's order are three different orders:
    // only a listing by the run's own start passes.
    state.session(
        "11111111-0000-4000-8000-000000000001",
        "anthropic",
        "a-model-read-off-the-wire",
        "2024-03-01T00:00:00Z",
        "final",
    );
    state.session(
        "22222222-0000-4000-8000-000000000002",
        "openai",
        "another-model-off-the-wire",
        "2024-01-01T00:00:00Z",
        "cancelled",
    );
    state.session(
        "33333333-0000-4000-8000-000000000003",
        "copilot",
        "a-third-model-off-the-wire",
        "2024-02-01T00:00:00Z",
        "final",
    );

    let listed = state.sindri(&["sessions", "list"]);

    assert_eq!(listed.status.code(), Some(0), "a listing is not a failure");
    let said = text(&listed.stdout);
    let guids: Vec<&str> = said
        .lines()
        .map(|line| line.split(' ').next().unwrap_or_default())
        .collect();
    assert_eq!(
        guids,
        [
            "11111111-0000-4000-8000-000000000001",
            "33333333-0000-4000-8000-000000000003",
            "22222222-0000-4000-8000-000000000002",
        ],
        "the newest run is named first: {said}"
    );
    assert!(
        said.lines().all(|line| line.contains("2024-")),
        "and each line says when its run started: {said}"
    );
    assert!(
        text(&listed.stderr).is_empty(),
        "nothing is said on stderr: {}",
        text(&listed.stderr)
    );
}

const FIRST: &str = "3f2b8c1a-4d5e-4f60-9a1b-000000000001";
const SECOND: &str = "3f2b8c1a-4d5e-4f60-9a1b-000000000002";
const THIRD: &str = "7a1c9d2e-6b7c-4d8e-8f90-000000000003";

impl State {
    fn three_runs() -> State {
        let state = State::empty();
        state.session(
            FIRST,
            "anthropic",
            "a-model-read-off-the-wire",
            "2024-03-01T00:00:00Z",
            "final",
        );
        state.session(
            SECOND,
            "openai",
            "another-model-off-the-wire",
            "2024-02-01T00:00:00Z",
            "cancelled",
        );
        state.session(
            THIRD,
            "copilot",
            "a-third-model-off-the-wire",
            "2024-01-01T00:00:00Z",
            "final",
        );
        state
    }

    fn first_run_records(&self) -> PathBuf {
        let directory = self.session(
            FIRST,
            "anthropic",
            "a-model-read-off-the-wire",
            "2024-03-01T00:00:00Z",
            "final",
        );
        self.transcript(
            &directory,
            &[
                json!({"kind": "turn", "turn": 1}),
                json!({"kind": "call", "turn": 1, "name": "bash", "arguments": "{\"command\":\"ls\"}"}),
                json!({"kind": "result", "turn": 1, "name": "bash", "text": "a-file", "bytes": 6, "elapsed_ms": 12}),
                json!({"kind": "assistant", "turn": 1, "text": "Hello world"}),
            ],
        );
        self.statuses(
            &directory,
            &[json!({"phase": "working", "message": "reading the file", "turn": 1, "workspace": "sindri"})],
        );
        directory
    }
}

#[test]
fn sessions_list_says_nothing_when_no_run_was_ever_recorded() {
    let state = State::empty();

    let listed = state.sindri(&["sessions", "list"]);

    assert_eq!(
        listed.status.code(),
        Some(0),
        "a state home with no sessions in it is an empty listing, not a failure"
    );
    assert!(
        listed.stdout.is_empty(),
        "and nothing at all reaches stdout: {:?}",
        text(&listed.stdout)
    );
}

#[test]
fn sessions_show_reads_one_run_back_from_its_whole_guid() {
    let state = State::empty();
    let directory = state.first_run_records();
    state.session(
        THIRD,
        "copilot",
        "a-third-model-off-the-wire",
        "2024-01-01T00:00:00Z",
        "final",
    );
    assert!(directory.is_dir(), "the fixture is on disk");

    let shown = state.sindri(&["sessions", "show", FIRST]);

    assert_eq!(
        shown.status.code(),
        Some(0),
        "reading a run back is not a failure"
    );
    let said = text(&shown.stdout);
    for fact in [
        FIRST,
        "anthropic",
        "a-model-read-off-the-wire",
        "bubblewrap",
        "/workspace",
        "2024-03-01T00:00:00Z",
        "final",
        "Hello world",
        "a-file",
        "reading the file",
    ] {
        assert!(said.contains(fact), "a reading shows {fact}: {said}");
    }
    assert!(!said.contains(THIRD), "and only the one session: {said}");
    assert!(
        text(&shown.stderr).is_empty(),
        "nothing is said on stderr: {}",
        text(&shown.stderr)
    );
}

#[test]
fn sessions_show_reads_a_run_back_from_an_unambiguous_prefix() {
    let state = State::three_runs();

    let shown = state.sindri(&["sessions", "show", "7a1c9d2e"]);

    assert_eq!(
        shown.status.code(),
        Some(0),
        "one session carries the prefix"
    );
    let said = text(&shown.stdout);
    assert!(
        said.contains(THIRD),
        "the prefix names this session: {said}"
    );
    assert!(!said.contains(FIRST), "and no other: {said}");
}

#[test]
fn sessions_show_with_no_id_reads_back_the_newest_run() {
    let state = State::three_runs();

    let shown = state.sindri(&["sessions", "show"]);

    assert_eq!(
        shown.status.code(),
        Some(0),
        "there is a newest run to read"
    );
    let said = text(&shown.stdout);
    assert!(
        said.contains(FIRST),
        "the newest run is the one shown: {said}"
    );
    assert!(
        !said.contains(SECOND) && !said.contains(THIRD),
        "and only that one: {said}"
    );
}

#[test]
fn an_ambiguous_prefix_is_a_usage_error_that_lists_the_candidates() {
    let state = State::three_runs();

    let refused = state.sindri(&["sessions", "show", "3f2b8c1a"]);

    assert_eq!(
        refused.status.code(),
        Some(2),
        "a prefix that names more than one session cannot be used (§15)"
    );
    assert!(
        refused.stdout.is_empty(),
        "and no session is shown: {:?}",
        text(&refused.stdout)
    );
    let said = text(&refused.stderr);
    assert!(
        said.contains(FIRST) && said.contains(SECOND),
        "the refusal lists every candidate: {said}"
    );
    assert!(
        !said.contains(THIRD),
        "and nothing the prefix does not carry: {said}"
    );
}

#[test]
fn a_prefix_no_session_carries_is_a_usage_error_that_names_it() {
    let state = State::three_runs();

    let refused = state.sindri(&["sessions", "show", "deadbeef"]);

    assert_eq!(
        refused.status.code(),
        Some(2),
        "an id no session carries cannot be used either (§15)"
    );
    assert!(
        refused.stdout.is_empty(),
        "and nothing at all reaches stdout: {:?}",
        text(&refused.stdout)
    );
    let said = text(&refused.stderr);
    assert!(
        said.contains("deadbeef"),
        "the refusal names the id: {said}"
    );
    assert_eq!(
        said.lines().count(),
        1,
        "and it lists no candidates, because there are none: {said}"
    );
}

#[test]
fn a_session_left_by_new_keeps_saying_running_because_it_did_not_stop() {
    let state = State::empty();
    state.session(
        THIRD,
        "copilot",
        "a-third-model-off-the-wire",
        "2024-01-01T00:00:00Z",
        "running",
    );

    let listed = state.sindri(&["sessions", "list"]);
    let shown = state.sindri(&["sessions", "show", THIRD]);

    assert_eq!(listed.status.code(), Some(0));
    assert!(
        text(&listed.stdout).contains("running"),
        "a left session is still running in a listing: {}",
        text(&listed.stdout)
    );
    assert_eq!(shown.status.code(), Some(0));
    let said = text(&shown.stdout);
    assert!(
        said.contains("stopped running"),
        "and when it is read back: {said}"
    );
    assert!(
        !said.contains("ended"),
        "the run is given no end it did not reach: {said}"
    );
}

#[test]
fn show_json_hands_on_the_files_in_their_own_shape() {
    let state = State::empty();
    let directory = state.session(
        FIRST,
        "anthropic",
        "a-model-read-off-the-wire",
        "2024-03-01T00:00:00Z",
        "final",
    );
    // A field this build never writes and a kind of entry it has no variant for: a newer `sindri` talking
    // to an older reader, and both survive only if the files are handed on as bytes rather than
    // re-encoded through this build's types.
    let document = std::fs::read_to_string(directory.join("session.json"))
        .expect("a session document")
        .replace(
            "\"stopped\": \"final\"",
            "\"stopped\": \"final\",\n  \"a-field-this-build-never-wrote\": true",
        );
    std::fs::write(directory.join("session.json"), &document).expect("the document writes");
    let transcript = concat!(
        "{\"text\": \"Hello world\", \"kind\": \"assistant\", \"turn\": 1}\n",
        "{\"kind\": \"note\", \"text\": \"a thing this build has no entry for\"}\n",
    );
    std::fs::write(directory.join("transcript.jsonl"), transcript).expect("a transcript");
    let statuses = "{\"phase\": \"working\", \"message\": \"reading the file\", \"turn\": 1, \"workspace\": \"sindri\"}\n";
    std::fs::write(directory.join("status.json"), statuses).expect("a status file");

    let shown = state.sindri(&["sessions", "show", FIRST, "--json"]);

    assert_eq!(
        shown.status.code(),
        Some(0),
        "reading a run back is not a failure"
    );
    assert!(
        text(&shown.stderr).is_empty(),
        "nothing is said on stderr: {}",
        text(&shown.stderr)
    );
    let said = text(&shown.stdout);
    let printed: Value = serde_json::from_str(&said).expect("stdout is one JSON object");
    assert_eq!(
        printed["session"]["session"], FIRST,
        "the object holds the session document under `session`: {said}"
    );
    assert_eq!(
        printed["session"]["a-field-this-build-never-wrote"], true,
        "a field this build never wrote is handed on, not dropped: {said}"
    );
    assert_eq!(
        printed["transcript"][1]["kind"], "note",
        "an entry this build has no variant for is handed on whole: {said}"
    );
    for line in transcript.lines() {
        assert!(
            said.contains(line),
            "each transcript line is the file's own bytes, key order and all: {line} in {said}"
        );
    }
    assert_eq!(
        printed["status"][0]["message"], "reading the file",
        "and the status documents are the file's too: {said}"
    );
    assert_eq!(
        printed["transcript"]
            .as_array()
            .expect("a list of entries")
            .len(),
        2,
        "one element per line the file holds: {said}"
    );
}

#[test]
fn show_json_leaves_out_a_line_that_is_not_json() {
    let state = State::empty();
    let directory = state.session(
        FIRST,
        "anthropic",
        "a-model-read-off-the-wire",
        "2024-03-01T00:00:00Z",
        "running",
    );
    std::fs::write(
        directory.join("transcript.jsonl"),
        "{\"kind\": \"turn\", \"turn\": 1}\n{\"kind\": \"assistant\", \"text\": \"Hel",
    )
    .expect("a torn transcript");

    let shown = state.sindri(&["sessions", "show", FIRST, "--json"]);

    assert_eq!(shown.status.code(), Some(0));
    let said = text(&shown.stdout);
    let printed: Value = serde_json::from_str(&said).expect("stdout is still one JSON object");
    assert_eq!(
        printed["transcript"]
            .as_array()
            .expect("a list of entries")
            .len(),
        1,
        "the whole line reads and the torn one is left out: {said}"
    );
}
