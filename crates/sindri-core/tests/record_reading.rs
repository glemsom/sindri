//! Proves a session directory reads back as the run it recorded: every session
//! the state home holds, newest first, and the one session a whole GUID or an
//! unambiguous prefix names, where a prefix that names more than one names none.
//!
//! Most directories here are written by hand rather than by [`Record::open`], so
//! the reader is proved against files no writer in this build produced: that is
//! what a session another version of `sindri` left behind looks like, and a
//! field the reader does not know has to cost nothing (§13, ADR-0007). One test
//! opens a record with this build's own writer and reads it back, because a
//! field the two spell differently would otherwise read as nothing at all. Every
//! state home is a temp directory of the test's own, so no real
//! `XDG_STATE_HOME` is read or written.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::json;
use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::provider::Route;
use sindri_core::records::{Facts, Record, choose, read_sessions};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Limits, State, Stop};
use sindri_core::session::{Session, sessions_root};
use sindri_core::wire::Wire;
use tempfile::TempDir;

/// A state home of the test's own, and the session directories written under it.
struct Ground {
    /// Held so the directory outlives the paths that name it.
    _root: TempDir,
    /// `$XDG_STATE_HOME`, as the reader is handed it.
    home: PathBuf,
}

impl Ground {
    /// A state home holding no session at all.
    fn empty() -> Ground {
        let root = tempfile::tempdir().expect("a temp state home");
        let home = root.path().join("state");
        std::fs::create_dir_all(&home).expect("a state home");
        Ground { _root: root, home }
    }

    /// Every variable the reader resolves a state home from.
    fn variables(&self) -> BTreeMap<String, String> {
        BTreeMap::from([(
            String::from("XDG_STATE_HOME"),
            self.home.display().to_string(),
        )])
    }

    /// `$XDG_STATE_HOME/sindri/sessions`, which is where every session lives.
    fn sessions(&self) -> PathBuf {
        sessions_root(&self.variables()).expect("a sessions root")
    }

    /// A session directory named `guid`, whose document says it started at
    /// `start` and stopped with `stopped`.
    fn session(&self, guid: &str, start: &str, stopped: &str) -> PathBuf {
        let directory = self.sessions().join(guid);
        std::fs::create_dir_all(directory.join("tmp")).expect("a session temp");
        let document = json!({
            "session": guid,
            "provider": "anthropic",
            "model": "a-model-read-off-the-wire",
            "enforcement": "bubblewrap",
            "policy": {"enforcement": "bubblewrap"},
            "workspace": "/workspace",
            "dials": {"max_turns": 24},
            "start": start,
            "turns": 2,
            "usage": {"input": 11, "output": 7},
            "stopped": stopped,
            "a-field-this-build-never-wrote": true,
        });
        std::fs::write(directory.join("session.json"), format!("{document:#}\n"))
            .expect("a session document");
        directory
    }
}

#[test]
fn every_session_the_state_home_holds_reads_newest_first() {
    let state = Ground::empty();
    // Named so the directory's own order and the GUID's order both differ from
    // the order the run's own start puts them in.
    state.session("1111-bbbb", "2024-03-01T00:00:00Z", "final");
    state.session("2222-aaaa", "2024-01-01T00:00:00Z", "final");
    state.session("3333-cccc", "2024-02-01T00:00:00Z", "cancelled");

    let read = read_sessions(&state.sessions()).expect("the state home reads");

    let guids: Vec<&str> = read.iter().map(|session| session.guid()).collect();
    assert_eq!(
        guids,
        ["1111-bbbb", "3333-cccc", "2222-aaaa"],
        "a listing is the run's own start, newest first"
    );
    assert_eq!(
        read[0].stopped(),
        "final",
        "and each session keeps the word its document carries"
    );
}

#[test]
fn a_directory_holding_no_readable_document_is_no_session_to_list() {
    let state = Ground::empty();
    state.session("4444-dddd", "2024-04-01T00:00:00Z", "final");
    std::fs::create_dir_all(state.sessions().join("5555-eeee").join("tmp"))
        .expect("a session temp");
    let unreadable = state.sessions().join("6666-ffff");
    std::fs::create_dir_all(&unreadable).expect("a session directory");
    std::fs::write(unreadable.join("session.json"), "{\"session\": ").expect("a torn document");

    let read = read_sessions(&state.sessions()).expect("the state home reads");

    assert_eq!(
        read.len(),
        1,
        "one session records a run, and the other two directories record none"
    );
    assert_eq!(read[0].guid(), "4444-dddd");
}

#[test]
fn a_whole_guid_names_its_own_session() {
    let state = Ground::empty();
    state.session("7777-abcd", "2024-05-01T00:00:00Z", "final");
    state.session("8888-abcd", "2024-04-01T00:00:00Z", "final");
    let sessions = read_sessions(&state.sessions()).expect("the state home reads");

    let chosen = choose(&sessions, "8888-abcd").expect("the whole GUID names one session");

    assert_eq!(chosen.guid(), "8888-abcd");
}

#[test]
fn a_prefix_one_session_carries_names_that_session() {
    let state = Ground::empty();
    state.session("9a9a-1111", "2024-05-01T00:00:00Z", "final");
    state.session("9b9b-2222", "2024-04-01T00:00:00Z", "final");
    let sessions = read_sessions(&state.sessions()).expect("the state home reads");

    let chosen = choose(&sessions, "9b9").expect("only one session carries the prefix");

    assert_eq!(chosen.guid(), "9b9b-2222");
}

#[test]
fn a_prefix_more_than_one_session_carries_names_none_and_lists_them() {
    let state = Ground::empty();
    state.session("0ab1-1111", "2024-05-01T00:00:00Z", "final");
    state.session("0ab1-2222", "2024-04-01T00:00:00Z", "final");
    state.session("0ac1-3333", "2024-03-01T00:00:00Z", "final");
    let sessions = read_sessions(&state.sessions()).expect("the state home reads");

    let refused = choose(&sessions, "0ab1-").expect_err("two sessions carry the prefix");

    assert_eq!(
        refused.candidates().join(", "),
        "0ab1-1111, 0ab1-2222",
        "the refusal lists every candidate, in the order a listing puts them"
    );
    assert_eq!(
        refused.exit_code(),
        2,
        "an id that names no one session is a usage error (§15)"
    );
}

#[test]
fn a_prefix_no_session_carries_is_absent() {
    let state = Ground::empty();
    state.session("0cd1-1111", "2024-05-01T00:00:00Z", "final");
    let sessions = read_sessions(&state.sessions()).expect("the state home reads");

    let refused = choose(&sessions, "0cd1-2222").expect_err("no session carries the prefix");

    assert_eq!(
        refused.exit_code(),
        2,
        "an absent id is a usage error too (§15)"
    );
    assert!(
        refused.to_string().contains("0cd1-2222"),
        "the refusal names the id as it was typed: {refused}"
    );
    assert!(
        refused.candidates().is_empty(),
        "and there is nothing to list"
    );
}

#[test]
fn a_run_that_was_recorded_reads_back_as_the_run_it_was() {
    // The reader against this build's own writer, which is the one shape no
    // hand-written fixture can stand in for: a field the two spell differently
    // would read as nothing at all.
    let state = Ground::empty();
    let workspace = state.home.join("workspace");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    let boundary = Boundary::from_config(&Config::default(), &workspace, Enforcement::Unsafe)
        .expect("the boundary holds");
    let limits = Limits::default();
    let secrets = SecretSet::default();
    let session = Session::create(&state.home).expect("a session");
    let mut record = Record::open(
        &session,
        Facts {
            wire: Wire::Copilot,
            model: "a-model-read-off-the-wire",
            // Copilot serves two endpoints and this name asks for neither, so
            // the route is the one its dialect answers by habit.
            route: Some(Route::Chat),
            boundary: &boundary,
            limits: &limits,
            workspace: &workspace,
            secrets: &secrets,
        },
    )
    .expect("a record");
    record.turn_started(1).expect("a turn");
    record.answered(1, "Hello world").expect("an answer");
    record
        .stopped(&State::Stopped(Stop::Final))
        .expect("a stop");
    record.finish().expect("the session ends");

    let sessions = read_sessions(&state.sessions()).expect("the state home reads");

    assert_eq!(sessions.len(), 1, "one run records one session");
    let read = &sessions[0];
    assert_eq!(read.guid(), session.guid(), "by the GUID the run reported");
    assert_eq!(read.wire(), Wire::Copilot);
    assert_eq!(read.model(), "a-model-read-off-the-wire");
    assert_eq!(read.enforcement(), Enforcement::Unsafe);
    assert_eq!(read.workspace(), workspace.as_path());
    assert_eq!(read.turns(), 1);
    assert_eq!(read.stopped(), "final", "a run that stopped says why");
    assert!(read.end().is_some(), "and when");
    assert_eq!(
        choose(&sessions, &session.guid()[..8])
            .expect("the run's own prefix names it")
            .guid(),
        session.guid()
    );
}
