//! The dials at the process seam (issue #18): a model changed in `/settings`
//! answers the next prompt on the thread the last one built, a key typed into
//! the modal lands in the config file and nowhere else, and a run recorded from
//! the frame reads back exactly as a batch run does.
//!
//! Every wait has a deadline and a machine without a pty skips cleanly: CI is
//! headless, and a frame that never draws must fail rather than hang (ADR-0011).

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use std::io::Write;

use common::pty::{CLEAR_FIELD, DOWN, ENTER, Pty, expect, spawn, type_line};
use common::{Env, TEST_KEY, stderr, stdout, write_config};

/// The fake credential a user types into the modal: shaped like a real one so a
/// leak is unmistakable, and belonging to nothing.
const TYPED_KEY: &str = "sk-typed-in-the-settings-modal-0f2c";

/// The config a TUI run starts from: the mock's wire, and the first model.
fn config_for(mock: &MockProvider, model: &str, key: Option<&str>) -> String {
    let mut toml = format!(
        "provider = \"anthropic\"\nmodel = \"{model}\"\nbase_url = \"{}\"\n",
        mock.base_url()
    );
    if let Some(key) = key {
        toml.push_str(&format!("api_key = \"{key}\"\n"));
    }
    toml
}

/// The environment a TUI run starts in. Only the base URL and a credential are
/// stated: the model is the file's, which is the thing under test.
fn spawn_with(env: &Env, pty: &Pty, mock: &MockProvider) -> std::process::Child {
    let base_url = mock.base_url();
    spawn(
        env,
        pty,
        &[
            ("SINDRI_API_KEY", TEST_KEY),
            ("SINDRI_BASE_URL", base_url.as_str()),
        ],
    )
}

/// Types into the settings modal: down to the field, clear it, and fill it.
fn set_field(pty: &Pty, downs: usize, value: &str) {
    // One write per cursor move: a `DOWN` split across reads is an `Esc` and
    // then two typed characters, and the modal is gone.
    let mut port = pty.port();
    for _ in 0..downs {
        port.write_all(DOWN).expect("a cursor move");
    }
    drop(port);
    pty.type_key(CLEAR_FIELD);
    for byte in value.bytes() {
        pty.type_key(byte);
    }
}

/// The same, read off the screen: a fact the rail changes one character of is
/// written once as a character, and only the screen puts the row back together.
fn expect_on_screen(pty: &Pty, child: &mut std::process::Child, want: &str) -> String {
    let drawn = pty.wait_for_screen(child, want);
    if !drawn.contains(want) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the screen never said {want:?}, drew: {drawn:?}");
    }
    drawn
}

#[test]
fn a_model_changed_in_settings_answers_the_next_prompt_on_the_same_thread() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the dials are not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::answer("the first answer"),
        Reply::answer("the second answer"),
    ]);
    write_config(&env, &config_for(&mock, "model-one", None));
    let mut child = spawn_with(&env, &pty, &mock);

    // One prompt, answered by the model the file names.
    expect(&pty, &mut child, "cache");
    type_line(&pty, "what changed?");
    expect(&pty, &mut child, "the first answer");

    // The dial: `/settings` → the model field → a new name → save.
    type_line(&pty, "/settings");
    expect(&pty, &mut child, "extra_writable_paths");
    set_field(&pty, 1, "model-two");
    pty.type_key(ENTER);
    expect(&pty, &mut child, "settings saved");

    // A second prompt, which must be a follow-up rather than a one-shot.
    type_line(&pty, "and then?");
    expect(&pty, &mut child, "the second answer");

    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "two prompts, two requests");
    assert_eq!(requests[0].json()["model"], "model-one");
    assert_eq!(
        requests[1].json()["model"],
        "model-two",
        "the next turn went to the model just saved, with no restart"
    );
    let second = requests[1].body.as_str();
    assert!(
        second.contains("what changed?") && second.contains("the first answer"),
        "and the thread survived the change: {second}"
    );
    let file = std::fs::read_to_string(env.config.join("sindri/config.toml")).expect("the config");
    assert!(
        file.contains("model = \"model-two\""),
        "the saved value is in the one config file: {file}"
    );
}

#[test]
fn a_saved_extra_path_is_in_force_for_the_very_next_command() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the saved path is not tested here");
        return;
    };
    let env = Env::new();
    // A directory beside the workspace, which no run may write until it is
    // saved as an extra.
    let scratch = env
        .workspace
        .parent()
        .expect("a workspace's parent")
        .join("scratch");
    std::fs::create_dir_all(&scratch).expect("scratch");
    let scratch = scratch.canonicalize().expect("canonical scratch");
    let mock = MockProvider::start(vec![Reply::answer("answered")]);
    write_config(&env, &config_for(&mock, "model-one", None));
    let mut child = spawn_with(&env, &pty, &mock);

    // The rail states the posture a run is bounded by: the workspace alone.
    let opened = expect_on_screen(&pty, &mut child, "bwrap · 1 writable");
    assert!(!opened.is_empty(), "the frame is drawing");
    type_line(&pty, "/settings");
    expect(&pty, &mut child, "extra_writable_paths");
    set_field(&pty, 7, &scratch.display().to_string());
    pty.type_key(ENTER);
    expect(&pty, &mut child, "settings saved");

    // The session was rebased, not rebuilt: the posture the next command runs
    // under is the one just saved (ADR-0015 §4).
    let screen = expect_on_screen(&pty, &mut child, "bwrap · 2 writable");
    // A rebase is the same session bounded by the settings just saved, not a
    // second one: the rail's GUID and the record on disk are still the launch's.
    let opened = only_session(&env);
    assert!(
        screen.contains(&opened[..8]),
        "the rail still shows the session the launch opened: {screen}"
    );
    let (code, _) = pty.quit(&mut child);
    assert_eq!(
        only_session(&env),
        opened,
        "saving settings opens no session"
    );
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let file = std::fs::read_to_string(env.config.join("sindri/config.toml")).expect("the config");
    assert!(
        file.contains(&scratch.display().to_string()),
        "the saved path is in the one config file: {file}"
    );
    let recorded = only_session(&env);
    let shown = shown(&env, &recorded);
    assert!(
        shown.contains("bubblewrap"),
        "and the record still says how the run was bounded: {shown}"
    );
}

#[test]
fn a_key_typed_in_the_modal_is_written_to_the_file_and_nowhere_else() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the secret's path is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("answered")]);
    write_config(&env, &config_for(&mock, "model-one", None));
    let mut child = spawn_with(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "/settings");
    // The key field is the fourth of the schema's nine, and it starts where
    // the run does: at the environment, never at a secret.
    let drawn = pty.wait_for_text(&mut child, "from environment");
    assert!(
        drawn.contains("api_key"),
        "the modal opened on the key field's row: {drawn:?}"
    );
    set_field(&pty, 3, TYPED_KEY);
    let warned = pty.wait_for_text(&mut child, "plaintext");
    assert!(
        warned.contains("plaintext") && warned.contains("config.toml"),
        "typing a key warns, naming the file: {warned:?}"
    );
    assert!(
        !warned.contains(TYPED_KEY),
        "and the value is masked, never echoed: {warned:?}"
    );
    pty.type_key(ENTER);
    expect(&pty, &mut child, "settings saved");

    // A run on the saved key, so the record has everything a run writes.
    type_line(&pty, "a question");
    expect(&pty, &mut child, "answered");
    let (code, leaving) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");
    assert!(
        !leaving.contains(TYPED_KEY),
        "the frame never drew the key, in any frame: {leaving:?}"
    );

    let file = std::fs::read_to_string(env.config.join("sindri/config.toml")).expect("the config");
    assert!(file.contains(TYPED_KEY), "the file holds the key: {file}");
    let record = everything_written(&env.state);
    assert!(
        !record.contains(TYPED_KEY),
        "the session record holds no key: {record}"
    );
    let shown = env
        .cmd()
        .args(["sessions", "show", &only_session(&env)])
        .assert();
    assert_eq!(shown.get_output().status.code(), Some(0));
    assert!(
        !stdout(shown.get_output()).contains(TYPED_KEY),
        "and neither does `sessions show`"
    );
}

/// Everything under `dir`, as text.
fn everything_written(dir: &std::path::Path) -> String {
    let mut all = String::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                all.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    all
}

/// The id of the one session directory a single run left behind.
fn only_session(env: &Env) -> String {
    let root = env.state.join("sindri/sessions");
    let mut found: Vec<String> = std::fs::read_dir(&root)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    assert_eq!(found.len(), 1, "one run, one record: {found:?}");
    found.remove(0)
}

#[test]
fn a_run_recorded_from_the_frame_reads_back_as_a_batch_run_does() {
    let Some(pty) = Pty::new() else {
        eprintln!("no pty on this machine: the record's read-back is not tested here");
        return;
    };
    let env = Env::new();
    let mock = MockProvider::start(vec![Reply::answer("twelve files changed")]);
    write_config(&env, &config_for(&mock, "model-one", None));
    let mut child = spawn_with(&env, &pty, &mock);

    expect(&pty, &mut child, "cache");
    type_line(&pty, "how many files changed?");
    expect(&pty, &mut child, "twelve files changed");
    let (code, _) = pty.quit(&mut child);
    assert_eq!(code, Some(0), "leaving the frame is not a failure");

    let from_tui = shown(&env, &only_session(&env));
    // The same question, asked by batch mode against the same mock, so the two
    // records are two recordings of the same thing.
    let batch = common::run_with(
        &env,
        &["-b", "how many files changed?"],
        &[
            ("SINDRI_BASE_URL", mock.base_url().as_str()),
            ("SINDRI_MODEL", "model-one"),
        ],
    );
    assert_eq!(batch.status.code(), Some(0), "batch: {}", stderr(&batch));
    let sessions = session_ids(&env);
    let from_batch = shown(&env, &sessions[1]);

    assert_eq!(
        entries(&from_tui),
        entries(&from_batch),
        "the frame's record reads exactly as the batch run's:\n{from_tui}\n---\n{from_batch}"
    );
    assert!(
        from_tui.contains("sandbox: bubblewrap") && from_tui.contains("stopped: final"),
        "and it is a record, header and all: {from_tui}"
    );
}

/// Every session id in this environment, oldest first.
fn session_ids(env: &Env) -> Vec<String> {
    let root = env.state.join("sindri/sessions");
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    found.sort_by_key(|path| {
        std::fs::metadata(path.join("session.json"))
            .and_then(|meta| meta.modified())
            .ok()
    });
    found
        .iter()
        .filter_map(|path| path.file_name()?.to_str().map(str::to_string))
        .collect()
}

/// What `sindri sessions show <id>` printed.
fn shown(env: &Env, id: &str) -> String {
    let output = env.cmd().args(["sessions", "show", id]).assert();
    let output = output.get_output();
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(output));
    stdout(output)
}

/// The transcript part of a `sessions show`: everything after the metadata.
/// Those are the lines that say what was asked and what was answered, and they
/// must not depend on which front end ran it.
fn entries(shown: &str) -> String {
    match shown.find("\n--- ") {
        Some(at) => shown[at..].to_string(),
        None => panic!("a record has a transcript: {shown}"),
    }
}
