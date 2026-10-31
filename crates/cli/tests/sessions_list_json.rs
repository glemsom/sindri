//! `sindri sessions list --json`: one compact object per line, the text row's
//! fields in the text row's order, so `jq` streams a list and a script reads
//! what a person reads.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::records::{NEWEST, Record, root, three, write};
use common::{Env, stderr, stdout};

fn objects(env: &Env) -> Vec<serde_json::Value> {
    let listed = env.cmd().args(["sessions", "list", "--json"]).assert();
    let listed = listed.get_output();
    assert_eq!(listed.status.code(), Some(0), "stderr: {}", stderr(listed));
    stdout(listed)
        .lines()
        .map(|line| serde_json::from_str(line).expect("one object per line"))
        .collect()
}

#[test]
fn json_is_one_object_per_line_with_the_fields_the_row_has() {
    let env = Env::new();
    three(&root(&env));

    let printed = objects(&env);
    assert_eq!(printed.len(), 3, "one object per record: {printed:?}");

    let unfinished = &printed[0];
    assert_eq!(unfinished["id"], NEWEST, "newest first, as the text row");
    assert_eq!(unfinished["model"], "claude-new");
    assert_eq!(unfinished["stopped"], "running");
    assert_eq!(unfinished["started_ms"], 1_700_000_120_000_u64);
    assert!(
        unfinished.get("turns").is_none() && unfinished.get("usage").is_none(),
        "a field the record does not have is left out, never null: {unfinished}"
    );

    let ended = &printed[2];
    assert_eq!(ended["turns"], 3);
    assert_eq!(ended["usage"]["total_tokens"], 1_290);
    assert_eq!(ended["prompt"], "how many files?");
}

#[test]
fn the_objects_come_in_the_text_rows_own_field_order() {
    let env = Env::new();
    three(&root(&env));

    let listed = env.cmd().args(["sessions", "list", "--json"]).assert();
    let text = stdout(listed.get_output());
    assert!(!text.contains("null"), "nothing is sent as a null: {text}");
    let oldest = text.lines().nth(2).expect("the oldest row");
    let mut at = 0;
    for field in [
        "id",
        "started_ms",
        "model",
        "turns",
        "stopped",
        "usage",
        "prompt",
    ] {
        let found = oldest
            .find(&format!("\"{field}\""))
            .unwrap_or_else(|| panic!("{field} is missing from {oldest}"));
        assert!(found >= at, "{field} is out of the row's order: {oldest}");
        at = found;
    }
}

#[test]
fn a_record_with_no_prompt_says_so_and_leaves_the_field_out_of_json() {
    let env = Env::new();
    write(
        &root(&env),
        &Record {
            id: NEWEST,
            started_ms: 1_700_000_120_000,
            model: "claude-new",
            turns: Some(1),
            stopped: "final",
            usage: None,
            prompt: None,
        },
    );

    let text = stdout(env.cmd().args(["sessions", "list"]).assert().get_output());
    assert!(
        text.trim_end().ends_with('—'),
        "a record with no prompt says so rather than showing nothing: {text:?}"
    );

    let object = objects(&env).remove(0);
    assert!(object.get("prompt").is_none(), "{object}");
}

/// A prompt long enough that a row has to cut it: the one field a fixed-width
/// table cannot hold.
const LONG: &str = "summarise every file under the workspace, then tell me which of them changed since the last release and what a reader should look at first";

#[test]
fn a_long_prompt_is_clipped_to_one_column_and_json_holds_it_whole() {
    let env = Env::new();
    write(
        &root(&env),
        &Record {
            id: NEWEST,
            started_ms: 1_700_000_120_000,
            model: "claude-new",
            turns: Some(1),
            stopped: "final",
            usage: None,
            prompt: Some(LONG),
        },
    );

    let text = stdout(env.cmd().args(["sessions", "list"]).assert().get_output());
    let row = text.trim_end();
    let column = row.rsplit("  ").next().expect("the prompt column");
    assert_eq!(
        column.chars().count(),
        60,
        "the prompt is clipped at one column's width: {column:?}"
    );
    assert!(
        column.ends_with('…'),
        "a cut prompt says it was cut: {column:?}"
    );
    assert!(LONG.starts_with(column.trim_end_matches('…')), "{column:?}");

    let object = objects(&env).remove(0);
    assert_eq!(
        object["prompt"], LONG,
        "`--json` holds the whole of it, so nothing is lost"
    );
}
