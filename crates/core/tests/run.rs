//! `Session`: the one module that starts a run. Proven at the
//! module's own seam — a run environment pointing at a temporary workspace and
//! state root, and a resolved config — so no process, no model and no bubblewrap
//! is involved in any of it.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a session that cannot start reports itself"
)]

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sindri_core::agent::{Report, Stopped};
use sindri_core::config::{Config, Provider};
use sindri_core::provider::{Error, Usage};
use sindri_core::record::Stored;
use sindri_core::sandbox::{Enforcement, PolicyError};
use sindri_core::session::{Env, Session, StartError};
use tempfile::TempDir;

/// A credential shaped like a real one but belonging to nothing.
const KEY: &str = "sk-ant-test-not-a-real-key";

fn host() -> (TempDir, Env) {
    let root = TempDir::new().expect("host");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let env = Env {
        workspace: workspace.canonicalize().expect("canonical workspace"),
        search_path: "/usr/bin:/bin".to_string(),
        state: root.path().join("state"),
    };
    (root, env)
}

fn config(extras: &[&Path]) -> Config {
    Config {
        provider: Provider::Anthropic,
        model: "claude-mock".to_string(),
        api_key: KEY.to_string(),
        base_url: None,
        max_turns: 8,
        timeout: Duration::from_secs(30),
        thinking: false,
        extra_writable: extras.iter().map(|path| path.to_path_buf()).collect(),
        enforcement: Enforcement::Bubblewrap,
        extra_headers: Vec::new(),
        refused_repo_config: None,
    }
}

fn report(stopped: Stopped) -> Report {
    Report {
        answer: "Twelve files.".to_string(),
        turns: 1,
        stopped,
        model: Some("claude-mock".to_string()),
        tool_calls: 0,
        usage: Usage::default(),
        duration: Duration::from_millis(20),
    }
}

fn recorded(env: &Env, session: &Session) -> PathBuf {
    env.state.join("sessions").join(session.dir().id())
}

#[test]
fn open_creates_the_session_directory_its_record_and_its_home() {
    let (_root, env) = host();
    let session = Session::open(&env, &config(&[])).expect("a session opens");

    let root = recorded(&env, &session);
    assert_eq!(
        session.dir().root(),
        root.as_path(),
        "the session lives under the run environment's state root"
    );
    assert!(
        root.join("session.json").is_file(),
        "the record was started"
    );
    assert!(
        root.join("tmp").join("home").is_dir(),
        "the session temp has the sandbox HOME a run needs"
    );
    assert!(!session.dir().id().is_empty(), "the session has a GUID");
    assert_eq!(
        session.policy().session_temp(),
        session.dir().temp(),
        "the policy maps the session temp the session created"
    );
    assert_eq!(
        session.policy().writable().paths(),
        [env.workspace.as_path()],
        "the workspace a run was opened in is the workspace it may write"
    );
    assert_eq!(
        session.policy().search_path(),
        env.search_path,
        "the sandbox is given the PATH the environment named, not the process's"
    );
}

#[test]
fn the_record_open_started_holds_the_meta_a_run_writes() {
    let (_root, env) = host();
    let session = Session::open(&env, &config(&[])).expect("a session opens");

    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    assert_eq!(stored.meta.id, session.dir().id());
    assert_eq!(stored.meta.provider, "anthropic");
    assert_eq!(stored.meta.model, "claude-mock");
    assert_eq!(
        stored.meta.enforcement, "bubblewrap",
        "the record states the sandbox the run had"
    );
    assert_eq!(stored.meta.pid, std::process::id());
    assert_eq!(stored.meta.stopped, "running", "nothing has ended yet");
    assert_eq!(stored.meta.ended_ms, None, "the record has no end");
    assert_eq!(stored.entries, [], "nothing has happened yet");
}

#[test]
fn finishing_a_cancelled_run_writes_a_cancelled_end() {
    let (_root, env) = host();
    let session = Session::open(&env, &config(&[])).expect("a session opens");

    session
        .finish(&Ok(report(Stopped::Cancelled)))
        .expect("the record takes its end");

    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    assert_eq!(stored.meta.stopped, "cancelled");
    assert_eq!(stored.meta.turns, Some(1));
    assert!(stored.meta.ended_ms.is_some(), "the end is stamped");
}

#[test]
fn finishing_a_failed_run_writes_the_error_the_shell_was_told() {
    let (_root, env) = host();
    let session = Session::open(&env, &config(&[])).expect("a session opens");

    let outcome = Err(Error::transport("the provider hung up"));
    session.finish(&outcome).expect("the record takes its end");

    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    assert_eq!(stored.meta.stopped, "error");
    assert_eq!(
        stored.meta.error.as_deref(),
        Some("the provider hung up"),
        "the word the shell saw is the word the record keeps"
    );
}

#[test]
fn a_record_that_cannot_be_written_is_reported_to_the_caller() {
    let (_root, env) = host();
    let session = Session::open(&env, &config(&[])).expect("a session opens");

    std::fs::remove_dir_all(session.dir().root()).expect("remove the session directory");

    assert!(
        session.finish(&Ok(report(Stopped::Final))).is_err(),
        "a finish that cannot write says so"
    );
}

#[test]
fn rebasing_gives_the_next_command_the_settings_that_were_saved() {
    let (root, env) = host();
    let extra = root.path().join("scratch");
    std::fs::create_dir_all(&extra).expect("extra path");
    let extra = extra.canonicalize().expect("canonical extra");
    let mut session = Session::open(&env, &config(&[])).expect("a session opens");

    session
        .rebase(&config(&[&extra]))
        .expect("the same session takes the new settings");

    assert_eq!(
        session.policy().writable().paths(),
        [env.workspace.as_path(), extra.as_path()],
        "the saved path is writable for the very next command"
    );
    let description = session.policy().describe();
    assert!(
        description.contains(&extra.display().to_string()),
        "the description names the saved path: {description}"
    );
    assert_eq!(
        session.policy().session_temp(),
        session.dir().temp(),
        "a rebase keeps the session it was opened as"
    );
}

#[test]
fn two_opens_are_two_sessions_and_the_first_is_left_without_an_end() {
    let (_root, env) = host();
    let first = Session::open(&env, &config(&[])).expect("the first session opens");
    let first_root = first.dir().root().to_path_buf();
    let second = Session::open(&env, &config(&[])).expect("the second session opens");

    assert_ne!(
        first.dir().id(),
        second.dir().id(),
        "each open is a session of its own"
    );
    let stored = Stored::load_in(first.dir()).expect("the first record reads back");
    assert_eq!(
        stored.meta.stopped, "running",
        "a superseded session did not stop, it was left"
    );
    assert_eq!(stored.meta.ended_ms, None, "and it gets no end written");
    assert!(
        first_root.join("session.json").is_file(),
        "the first record is untouched where it was written"
    );
}

#[test]
fn a_start_error_names_what_failed_and_carries_its_path() {
    let directory = StartError::Directory {
        path: PathBuf::from("/state/sindri/sessions"),
        source: io::Error::other("no space left"),
    };
    assert!(
        directory.to_string().contains("/state/sindri/sessions"),
        "the directory is named: {directory}"
    );

    let record = StartError::Record {
        path: PathBuf::from("/state/sindri/sessions/guid"),
        source: io::Error::other("read-only"),
    };
    assert!(
        record.to_string().contains("/state/sindri/sessions/guid"),
        "the record's path is named: {record}"
    );

    let sandbox = StartError::Sandbox(PolicyError::NotAbsolute {
        path: PathBuf::from("relative"),
    });
    assert!(
        sandbox.to_string().contains("relative"),
        "the sandbox's own words survive: {sandbox}"
    );
}

#[test]
fn a_workspace_the_policy_refuses_starts_no_record() {
    let (_root, env) = host();
    let missing = env.workspace.join("gone");

    let Err(error) = Session::open(&env, &config(&[&missing])) else {
        panic!("a path that is not there cannot be writable");
    };
    assert!(
        matches!(error, StartError::Sandbox(_)),
        "a refused policy is the sandbox's failure: {error}"
    );
    assert!(
        error.to_string().contains("gone"),
        "the path that was refused is named: {error}"
    );
    assert_eq!(
        records(&env),
        Vec::<PathBuf>::new(),
        "a session that never started leaves no record saying `running`"
    );
}

fn records(env: &Env) -> Vec<PathBuf> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.file_name().is_some_and(|name| name == "session.json") {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(&env.state, &mut found);
    found
}

#[test]
fn a_session_states_the_enforcement_the_config_hands_it_and_never_one_of_its_own() {
    let (_root, env) = host();
    let mut unsafe_config = config(&[]);
    unsafe_config.enforcement = Enforcement::YoloUnsafe;

    let session = Session::open(&env, &unsafe_config).expect("a session opens");

    assert_eq!(
        session.policy().enforcement(),
        Enforcement::YoloUnsafe,
        "the policy the session runs is the one the config named"
    );
    let stored = Stored::load_in(session.dir()).expect("the record reads back");
    assert_eq!(
        stored.meta.enforcement, "unsafe",
        "the record states the policy the run really had"
    );
}
