#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

//! `sindri sessions show`: one record read back — the newest when no id is
//! named, the one a prefix names, and either as text or as the files' own JSON.
//! The fixtures are directories a run could have left, so a reader is proven
//! against the record rather than against the writer.

mod common;

use common::provider::{MockProvider, Reply};
use common::records::{DETAILED, MIDDLE, NEWEST, OLDEST, SPILL, detailed, root, three};
use common::{Env, batch, sessions, stderr, stdout};

/// The id one run printed, so the directory a test reads is the one it made.
fn id_of(output: &std::process::Output) -> String {
    serde_json::from_str::<serde_json::Value>(stdout(output).trim_end())
        .expect("the object")["session"]
        .as_str()
        .expect("the id")
        .to_string()
}

#[test]
fn show_with_no_id_prints_the_newest_record() {
    let env = Env::new();
    three(&root(&env));

    let shown = env.cmd().args(["sessions", "show"]).assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));
    assert_eq!(stderr(shown), "", "a readable record is not an error");

    let text = stdout(shown);
    assert!(
        text.contains(&format!("session {NEWEST}")),
        "the run you just did is the one it shows:\n{text}"
    );
    assert!(
        !text.contains(MIDDLE) && !text.contains(OLDEST),
        "and only that one:\n{text}"
    );
}

/// The whole point of the default: after two runs the one you meant is the
/// second, and `list` agrees about which that is.
#[test]
fn after_two_runs_show_with_no_id_is_the_second() {
    let env = Env::new();
    let mock = MockProvider::start(vec![
        Reply::answer("the first"),
        Reply::answer("the second"),
    ]);

    let first = batch(&env, &mock, &["-b", "one", "--format", "json"], "");
    assert_eq!(first.status.code(), Some(0), "stderr: {}", stderr(&first));
    let first = id_of(&first);
    let second = batch(&env, &mock, &["-b", "two", "--format", "json"], "");
    assert_eq!(second.status.code(), Some(0), "stderr: {}", stderr(&second));
    let second = id_of(&second);
    assert_ne!(first, second, "two runs, two records");

    let shown = env.cmd().args(["sessions", "show"]).assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));
    let text = stdout(shown);
    assert!(
        text.contains(&format!("session {second}")),
        "the second run is the newest:\n{text}"
    );
    assert!(
        !text.contains(&first),
        "and the first is not shown:\n{text}"
    );

    let listed = env.cmd().args(["sessions", "list"]).assert();
    let listed = listed.get_output();
    let first_row = stdout(listed)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    assert!(
        first_row.contains(&second),
        "the listing agrees about which run is last: {first_row}"
    );
}

#[test]
fn a_prefix_names_the_one_record_it_starts() {
    let env = Env::new();
    three(&root(&env));

    let shown = env
        .cmd()
        .args(["sessions", "show", "0f1c8f2e-3333"])
        .assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));
    assert!(
        stdout(shown).contains(&format!("session {NEWEST}")),
        "four characters name the run: {}",
        stdout(shown)
    );
}

#[test]
fn an_ambiguous_prefix_exits_two_and_names_every_candidate() {
    let env = Env::new();
    three(&root(&env));

    let shown = env.cmd().args(["sessions", "show", "0f1c8f2e"]).assert();
    let shown = shown.get_output();
    assert_eq!(
        shown.status.code(),
        Some(2),
        "two records start with that prefix"
    );
    assert_eq!(stdout(shown), "", "and nothing is printed for it");
    let complaint = stderr(shown);
    for candidate in [NEWEST, MIDDLE, OLDEST] {
        assert!(
            complaint.contains(candidate),
            "an ambiguous prefix names its candidates, and {candidate} is one: {complaint}"
        );
    }
}

/// A prefix no record starts is a record that is not there: the same usage
/// error an id that was never written raises, whether or not it could be one.
#[test]
fn a_prefix_no_record_starts_exits_two_as_missing() {
    let env = Env::new();
    three(&root(&env));

    for wanted in ["deadbeef", "0f1c8f2e-4444-4a2b-8c3d-000000000009"] {
        let missing = env.cmd().args(["sessions", "show", wanted]).assert();
        let missing = missing.get_output();
        assert_eq!(missing.status.code(), Some(2), "no such session: {wanted}");
        assert_eq!(stdout(missing), "", "and nothing is printed for it");
        assert!(
            stderr(missing).contains(wanted),
            "but it says what it looked for: {}",
            stderr(missing)
        );
    }
}

#[test]
fn a_path_is_never_a_prefix() {
    let env = Env::new();
    three(&root(&env));

    let climbing = env.cmd().args(["sessions", "show", "../../etc"]).assert();
    let climbing = climbing.get_output();
    assert_eq!(climbing.status.code(), Some(2), "a path is not an id");
    assert_eq!(stdout(climbing), "");
    assert!(
        stderr(climbing).contains("is not a session id"),
        "the guard is what refused it: {}",
        stderr(climbing)
    );
    assert_eq!(sessions(&env).len(), 3, "and nothing was read or written");
}

#[test]
fn json_is_the_files_in_their_own_shape() {
    let env = Env::new();
    detailed(&root(&env));

    let shown = env.cmd().args(["sessions", "show", "--json"]).assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));
    assert_eq!(stderr(shown), "", "a readable record is not an error");

    let printed = stdout(shown);
    assert_eq!(
        printed.lines().count(),
        1,
        "one line, like `--format json`: {printed}"
    );
    let object: serde_json::Value = serde_json::from_str(&printed).expect("the object");
    assert_eq!(object["meta"]["id"], DETAILED, "{object}");
    assert_eq!(
        object["meta"]["started_ms"], 1_700_000_180_000u64,
        "the metadata is the file's own document: {object}"
    );
    assert_eq!(
        object["meta"]["policy"], "read the workspace, write nowhere",
        "the boundary the run ended under: {object}"
    );

    let kinds: Vec<&str> = object["entries"]
        .as_array()
        .expect("the transcript's entries")
        .iter()
        .map(|entry| entry["type"].as_str().expect("a typed entry"))
        .collect();
    assert_eq!(
        kinds,
        vec!["user", "assistant", "tool_call", "tool_result", "assistant"],
        "the entries are the transcript's lines, in order: {object}"
    );
    let result = &object["entries"][3];
    for (field, value) in [
        ("outcome", serde_json::json!("exited")),
        ("exit_code", serde_json::json!(0)),
        ("duration_ms", serde_json::json!(37)),
        ("truncated", serde_json::json!(true)),
        ("spill", serde_json::json!([SPILL])),
    ] {
        assert_eq!(
            result[field], value,
            "one call's facts, as fields: {result}"
        );
    }
    assert_eq!(
        object["entries"][1]["usage"]["input_tokens"], 120,
        "a turn's cost is on the turn: {object}"
    );

    let status = object["status"].as_array().expect("the status documents");
    assert_eq!(status.len(), 2, "every line of status.json: {object}");
    assert_eq!(
        status[0]["document"]["child-pid"], 4242,
        "bubblewrap's own document, unparaphrased: {status:?}"
    );
    assert_eq!(
        status[1]["line"], "bwrap: the sandbox started",
        "and a line that is not JSON keeps its own spelling: {status:?}"
    );
}

#[test]
fn json_leaves_status_out_when_the_record_has_none() {
    let env = Env::new();
    three(&root(&env));

    let shown = env
        .cmd()
        .args(["sessions", "show", NEWEST, "--json"])
        .assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));

    let object: serde_json::Value =
        serde_json::from_str(stdout(shown).trim_end()).expect("the object");
    assert!(object["meta"].is_object(), "{object}");
    assert!(object["entries"].is_array(), "{object}");
    assert!(
        object.get("status").is_none(),
        "a record without status.json has no status, rather than null: {object}"
    );
}

#[test]
fn the_text_render_carries_what_the_record_now_holds() {
    let env = Env::new();
    detailed(&root(&env));

    let shown = env.cmd().args(["sessions", "show", DETAILED]).assert();
    let shown = shown.get_output();
    assert_eq!(shown.status.code(), Some(0), "stderr: {}", stderr(shown));

    let text = stdout(shown);
    for wanted in [
        // #69's dials and boundary, which the header already stated.
        "dials: max turns 250, thinking on",
        // #67: the turn's own counters and the span it took.
        "turn cost: 2100 ms, input 120, output 33, total 153",
        "turn cost: 900 ms",
        // #67: the call's outcome, exit code, span and the text it spilled to.
        "exited (0), 37 ms, truncated, spill tmp/spill/t1.txt",
        // #66: the line the crash took, and what bwrap said.
        "1 line could not be read",
        "--- sandbox status ---",
        "\"child-pid\":4242",
        "bwrap: the sandbox started",
    ] {
        assert!(
            text.contains(wanted),
            "the render states {wanted:?}, and does not:\n{text}"
        );
    }
}

/// The exit codes do not move: a wrong id is the user's mistake, a record that
/// cannot be read is the environment's, and both shapes say the same thing.
#[test]
fn json_exits_the_way_the_text_render_does() {
    let env = Env::new();
    let broken = env.state.join("sindri/sessions/abcdef0123456789");
    std::fs::create_dir_all(&broken).expect("a session directory");
    std::fs::write(broken.join("session.json"), "{\"id\"").expect("broken metadata");

    let broken = env
        .cmd()
        .args(["sessions", "show", "abcdef0123456789", "--json"])
        .assert();
    let broken = broken.get_output();
    assert_eq!(broken.status.code(), Some(3), "no metadata, no record");
    assert_eq!(stdout(broken), "", "and no half an object is printed");
    assert!(
        stderr(broken).contains("session.json"),
        "the refusal names the file: {}",
        stderr(broken)
    );

    let missing = env
        .cmd()
        .args(["sessions", "show", "deadbeef", "--json"])
        .assert();
    let missing = missing.get_output();
    assert_eq!(missing.status.code(), Some(2), "no such session");
    assert_eq!(stdout(missing), "", "and no half an object is printed");
    assert!(
        stderr(missing).contains("deadbeef"),
        "and it says which: {}",
        stderr(missing)
    );
}
