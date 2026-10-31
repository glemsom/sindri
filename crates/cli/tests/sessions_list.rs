//! `sindri sessions list`: the records a state directory holds, one row each,
//! newest first. The fixtures are directories a run could have left, so the
//! listing is proven without starting a run, and nothing here writes a record.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::records::{MIDDLE, NEWEST, OLDEST, Record, root, three, write};
use common::{Env, stderr, stdout};

/// The date a row carries, in the one format a record is dated in: proven to be
/// a date here, and proven to be the right moment in `core/tests/clock.rs`.
fn dated(row: &str) -> bool {
    let field = row.split("  ").nth(1).unwrap_or_default();
    let (day, clock) = field.split_once(' ').unwrap_or((field, ""));
    let numeric = |part: &str| part.chars().all(|c| c.is_ascii_digit());
    let day: Vec<&str> = day.split('-').collect();
    let clock: Vec<&str> = clock.split(':').collect();
    day.len() == 3
        && clock.len() == 2
        && day.iter().all(|part| numeric(part))
        && clock.iter().all(|part| numeric(part))
        && day.iter().map(|part| part.len()).eq([4, 2, 2])
        && clock.iter().map(|part| part.len()).eq([2, 2])
}

#[test]
fn three_records_are_listed_newest_first_with_what_each_row_carries() {
    let env = Env::new();
    three(&root(&env));

    let listed = env.cmd().args(["sessions", "list"]).assert();
    let listed = listed.get_output();
    assert_eq!(listed.status.code(), Some(0), "stderr: {}", stderr(listed));
    assert_eq!(stderr(listed), "", "a listing is not an error");

    let text = stdout(listed);
    let rows: Vec<&str> = text.lines().collect();
    assert_eq!(rows.len(), 3, "one row per record:\n{text}");

    let newest = rows[0];
    for wanted in [
        NEWEST,
        "claude-new",
        "running",
        "— turns",
        "— tokens",
        "why is the gate red?",
    ] {
        assert!(
            newest.contains(wanted),
            "the newest row says {wanted:?}: {newest}"
        );
    }
    assert!(
        dated(newest),
        "when it started, in the one date format: {newest}"
    );

    let middle = rows[1];
    for wanted in [MIDDLE, "claude-middle", "7 turns", "max_turns", "47 tokens"] {
        assert!(
            middle.contains(wanted),
            "the middle row says {wanted:?}: {middle}"
        );
    }

    let oldest = rows[2];
    for wanted in [
        OLDEST,
        "claude-old",
        "3 turns",
        "final",
        "1290 tokens",
        "how many files?",
    ] {
        assert!(
            oldest.contains(wanted),
            "the oldest row says {wanted:?}: {oldest}"
        );
    }
    assert!(
        !text.contains("0 turns"),
        "a run with no turn count invents no zero:\n{text}"
    );
}

#[test]
fn a_row_counts_one_turn_and_invents_no_zero() {
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
            prompt: Some("how many files?"),
        },
    );

    let text = stdout(env.cmd().args(["sessions", "list"]).assert().get_output());
    assert!(
        text.contains("1 turn ") && !text.contains("1 turns"),
        "one turn is not `1 turns`: {text}"
    );
}

#[test]
fn a_limit_cuts_the_list_and_none_is_no_limit_at_all() {
    let env = Env::new();
    three(&root(&env));

    let one = env
        .cmd()
        .args(["sessions", "list", "--limit", "1"])
        .assert();
    let one = one.get_output();
    assert_eq!(one.status.code(), Some(0), "stderr: {}", stderr(one));
    let rows: Vec<String> = stdout(one).lines().map(str::to_string).collect();
    assert_eq!(rows.len(), 1, "one row for one: {}", stdout(one));
    assert!(
        rows[0].contains(NEWEST),
        "the newest row is the one kept: {}",
        rows[0]
    );

    let none = env
        .cmd()
        .args(["sessions", "list", "--limit", "0"])
        .assert();
    let none = none.get_output();
    assert_eq!(none.status.code(), Some(0), "stderr: {}", stderr(none));
    assert_eq!(stdout(none), "", "--limit 0 is a list with nothing in it");
}

#[test]
fn a_directory_that_is_not_a_record_is_skipped_and_the_rows_around_it_print() {
    let env = Env::new();
    let root = root(&env);
    three(&root);
    std::fs::create_dir_all(root.join("not-a-record")).expect("a stray directory");
    // A record whose metadata does not parse is no record at all: a listing
    // must not fall over on it.
    let broken = root.join("0f1c8f2e-4444-4a2b-8c3d-000000000004");
    std::fs::create_dir_all(&broken).expect("a session directory");
    std::fs::write(broken.join("session.json"), "{\"id\"").expect("broken metadata");

    let listed = env.cmd().args(["sessions", "list"]).assert();
    let listed = listed.get_output();
    assert_eq!(listed.status.code(), Some(0), "stderr: {}", stderr(listed));
    let text = stdout(listed);
    assert_eq!(
        text.lines().count(),
        3,
        "the rows around it still print:\n{text}"
    );
    assert!(!text.contains("not-a-record"), "{text}");
}

#[test]
fn an_empty_or_missing_state_directory_lists_nothing_and_is_not_a_failure() {
    let env = Env::new();
    let listed = env.cmd().args(["sessions", "list"]).assert();
    let listed = listed.get_output();
    assert_eq!(
        listed.status.code(),
        Some(0),
        "a state directory no run has written to: {}",
        stderr(listed)
    );
    assert_eq!(
        stdout(listed),
        "",
        "a list with nothing in it prints nothing"
    );
    assert_eq!(stderr(listed), "");

    // A directory with no records in it says the same.
    std::fs::create_dir_all(root(&env)).expect("an empty state directory");
    let empty = env.cmd().args(["sessions", "list"]).assert();
    let empty = empty.get_output();
    assert_eq!(empty.status.code(), Some(0), "stderr: {}", stderr(empty));
    assert_eq!(stdout(empty), "");
}

#[test]
fn a_state_directory_that_cannot_be_read_exits_three() {
    let env = Env::new();
    std::fs::create_dir_all(env.state.join("sindri")).expect("the state directory");
    // Not a directory, so not a listing: the one failure a listing can have.
    std::fs::write(env.state.join("sindri/sessions"), "not a directory").expect("a file");

    let listed = env.cmd().args(["sessions", "list"]).assert();
    let listed = listed.get_output();
    assert_eq!(
        listed.status.code(),
        Some(3),
        "an unreadable state directory is the environment's failure"
    );
    assert_eq!(stdout(listed), "", "and nothing is printed for it");
    assert!(
        stderr(listed).contains("sessions"),
        "the refusal names what it could not read: {}",
        stderr(listed)
    );
}
