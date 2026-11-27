use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

use super::*;
use tempfile::TempDir;

fn environment(vars: &[(&str, &str)]) -> BTreeMap<String, String> {
    vars.iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn state_home() -> TempDir {
    tempfile::tempdir().expect("a temp state home")
}

fn file(state: &TempDir) -> std::path::PathBuf {
    state.path().join("sindri").join("recall.jsonl")
}

fn in_the_state_home(state: &TempDir) -> BTreeMap<String, String> {
    let path = state.path().to_str().expect("a printable path");
    environment(&[("XDG_STATE_HOME", path)])
}

fn written(state: &TempDir) -> String {
    std::fs::read_to_string(file(state)).expect("the recall's file")
}

fn prompts(recall: &Recall) -> Vec<&str> {
    recall.prompts().collect()
}

fn asked(count: usize) -> String {
    (0..count)
        .map(|ask| format!("{{\"at\":\"2026-02-11T09:14:03Z\",\"prompt\":\"ask {ask}\"}}\n"))
        .collect()
}

fn put(state: &TempDir, lines: &str) {
    std::fs::create_dir_all(file(state).parent().expect("a directory")).expect("the directory");
    std::fs::write(file(state), lines).expect("a recall an earlier launch wrote");
}

#[test]
fn the_file_beside_the_sessions_is_what_a_recall_loads() {
    let state = state_home();
    std::fs::create_dir_all(file(&state).parent().expect("a directory")).expect("the directory");
    std::fs::write(
        file(&state),
        "{\"at\":\"2026-02-11T09:14:03Z\",\"prompt\":\"why does the retry test flake?\"}\n\
         {\"at\":\"2026-02-11T09:20:00Z\",\"prompt\":\"and the second one\"}\n",
    )
    .expect("a recall written by an earlier launch");

    let recall = Recall::load(&in_the_state_home(&state));

    assert_eq!(
        prompts(&recall),
        ["why does the retry test flake?", "and the second one"],
        "the entries are the lines the file holds, in the order they were asked"
    );
    assert_eq!(
        recall.entries[0].at, "2026-02-11T09:14:03Z",
        "and each keeps the moment the file said it was asked"
    );
}

#[test]
fn a_prompt_appended_is_a_line_in_the_file_and_an_entry_in_the_next_load() {
    let state = state_home();
    let mut recall = Recall::load(&in_the_state_home(&state));

    recall.append("why does the retry test flake?");

    assert_eq!(
        prompts(&recall),
        ["why does the retry test flake?"],
        "what the launch sent is what the recall holds at once"
    );
    let line: serde_json::Value =
        serde_json::from_str(written(&state).trim_end()).expect("one JSON object per line");
    assert_eq!(line["prompt"], "why does the retry test flake?");
    assert!(
        line["at"]
            .as_str()
            .is_some_and(|at| at.ends_with('Z') && at.contains('T')),
        "the line carries the moment it was asked: {line}"
    );
    assert_eq!(
        prompts(&Recall::load(&in_the_state_home(&state))),
        ["why does the retry test flake?"],
        "and the next launch reads it back"
    );
}

#[test]
fn a_state_home_with_no_file_loads_empty() {
    let state = state_home();

    assert_eq!(prompts(&Recall::load(&in_the_state_home(&state))), [""; 0]);
}

#[test]
fn no_state_directory_and_no_home_load_empty() {
    assert_eq!(prompts(&Recall::load(&environment(&[]))), [""; 0]);
    assert_eq!(
        prompts(&Recall::load(&environment(&[
            ("XDG_STATE_HOME", ""),
            ("HOME", "")
        ]))),
        [""; 0]
    );
}

#[test]
fn a_prompt_the_newest_entry_already_holds_is_not_stored_twice() {
    let state = state_home();
    let mut recall = Recall::load(&in_the_state_home(&state));

    recall.append("why does the retry test flake?");
    recall.append("why does the retry test flake?");

    assert_eq!(
        prompts(&recall),
        ["why does the retry test flake?"],
        "a repeated ask costs one stroke to walk past and not two"
    );
    assert_eq!(
        written(&state).lines().count(),
        1,
        "and the file keeps one line for it"
    );
    assert_eq!(
        prompts(&Recall::load(&in_the_state_home(&state))),
        ["why does the retry test flake?"],
        "so the next launch walks past it once"
    );
}

#[test]
fn a_prompt_an_older_entry_already_holds_keeps_its_own_place_in_time() {
    let state = state_home();
    let mut recall = Recall::load(&in_the_state_home(&state));

    recall.append("the first ask");
    recall.append("the second ask");
    recall.append("the first ask");

    assert_eq!(
        prompts(&recall),
        ["the first ask", "the second ask", "the first ask"],
        "walking back reads as the order things were asked"
    );
    assert_eq!(
        written(&state).lines().count(),
        3,
        "and the file says so too"
    );
}

#[test]
fn a_prompt_past_the_ceiling_is_stored_nowhere() {
    let state = state_home();
    let mut recall = Recall::load(&in_the_state_home(&state));

    recall.append(&"a line of a pasted log\n".repeat(5_000));
    recall.append(&"a long ask that is still an ask ".repeat(60));

    assert_eq!(
        prompts(&recall).len(),
        1,
        "a pasted log is no stroke an arrow key should have to walk past"
    );
    assert_eq!(
        written(&state).lines().count(),
        1,
        "and the file carries no line for it"
    );
}

#[test]
fn a_file_past_the_cap_is_rewritten_to_the_newest_of_its_entries_at_load() {
    let state = state_home();
    put(&state, &asked(6_000));

    let recall = Recall::load(&in_the_state_home(&state));

    let kept = prompts(&recall);
    assert!(
        kept.len() < 6_000,
        "a launch reads a bounded number of entries whatever the file holds"
    );
    assert_eq!(
        kept.last().copied(),
        Some("ask 5999"),
        "the newest ask is kept"
    );
    assert_eq!(
        kept.first().copied(),
        Some(format!("ask {}", 6_000 - kept.len()).as_str()),
        "and the oldest are the ones dropped"
    );
    assert_eq!(
        written(&state).lines().count(),
        kept.len(),
        "loading is where the file is rewritten"
    );
    assert_eq!(
        prompts(&Recall::load(&in_the_state_home(&state))),
        kept,
        "so the launch after it trims nothing"
    );
}

#[test]
fn a_send_to_a_file_at_the_cap_is_an_append_and_no_rewrite() {
    let state = state_home();
    put(&state, &asked(6_000));
    let mut recall = Recall::load(&in_the_state_home(&state));
    let capped = prompts(&recall).len();

    recall.append("one more ask");

    assert_eq!(
        written(&state).lines().count(),
        capped + 1,
        "a send is a line appended and never a file rewritten"
    );
    assert_eq!(
        prompts(&recall).len(),
        capped,
        "and the recall holds its bound all the same"
    );
    assert_eq!(
        prompts(&recall).last().copied(),
        Some("one more ask"),
        "the newest ask is the one the launch just sent"
    );
}

#[test]
fn a_line_that_does_not_parse_is_skipped_and_the_launch_recalls_the_rest() {
    let state = state_home();
    put(
        &state,
        "{\"at\":\"2026-02-11T09:14:03Z\",\"prompt\":\"the ask before\"}\n\
         not a line a launch wrote\n\
         {\"at\":\"2026-02-11T09:20:00Z\",\"prompt\":\"the ask after\"}\n\n",
    );

    let recall = Recall::load(&in_the_state_home(&state));

    assert_eq!(
        prompts(&recall),
        ["the ask before", "the ask after"],
        "the file is the reader's to break by hand, and a broken line costs no launch"
    );
    assert_eq!(
        written(&state).lines().count(),
        4,
        "a launch that only reads leaves the line it cannot parse where it is"
    );
}

#[test]
fn the_file_is_created_readable_and_writable_by_its_owner_alone() {
    let state = state_home();
    let mut recall = Recall::load(&in_the_state_home(&state));

    recall.append("a word of my own");

    let mode = std::fs::metadata(file(&state))
        .expect("the recall's file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "no other account on the machine reads the reader's words"
    );
}

#[test]
fn a_file_wider_than_the_reader_alone_is_tightened_at_load() {
    let state = state_home();
    put(
        &state,
        "{\"at\":\"2026-02-11T09:14:03Z\",\"prompt\":\"an ask\"}\n",
    );
    std::fs::set_permissions(file(&state), std::fs::Permissions::from_mode(0o644))
        .expect("a file an earlier version left readable");

    let recall = Recall::load(&in_the_state_home(&state));

    assert_eq!(prompts(&recall), ["an ask"], "and the words are still read");
    let mode = std::fs::metadata(file(&state))
        .expect("the recall's file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "a launch that only reads closes a file the reader's words should never have opened"
    );
}

#[test]
fn a_recall_with_nowhere_to_write_still_recalls_everything_the_launch_sent() {
    let state = state_home();
    std::fs::write(state.path().join("sindri"), "not a directory")
        .expect("a state home to trip on");

    let mut recall = Recall::load(&in_the_state_home(&state));
    recall.append("the first ask");
    recall.append("the second ask");
    let mut nowhere = Recall::none();
    nowhere.append("what a launch with no state home asked");

    assert_eq!(
        prompts(&recall),
        ["the first ask", "the second ask"],
        "a state home that cannot be written is no refusal, and the launch recalls what it sent"
    );
    assert_eq!(
        prompts(&nowhere),
        ["what a launch with no state home asked"],
        "and so is a recall handed no path at all"
    );
}

#[test]
fn a_multi_line_prompt_is_one_line_of_the_file_and_whole_in_the_next_load() {
    let state = state_home();
    let mut recall = Recall::load(&in_the_state_home(&state));

    recall.append("the first line\nand the second\nand a third");

    assert_eq!(
        written(&state).lines().count(),
        1,
        "one object per line, whatever shape the prompt's words take"
    );
    assert_eq!(
        prompts(&Recall::load(&in_the_state_home(&state))),
        ["the first line\nand the second\nand a third"],
        "and the prompt's lines are recalled whole"
    );
}

#[test]
fn two_launches_appending_at_once_keep_each_others_lines() {
    use std::sync::{Arc, Barrier};

    let state = state_home();
    let home = in_the_state_home(&state);
    let gate = Arc::new(Barrier::new(2));
    let launches: Vec<_> = (0..2)
        .map(|launch| {
            let home = home.clone();
            let gate = Arc::clone(&gate);
            std::thread::spawn(move || {
                let mut recall = Recall::load(&home);
                gate.wait();
                for ask in 0..100 {
                    recall.append(&format!("ask {launch}-{ask}"));
                }
            })
        })
        .collect();
    for launch in launches {
        launch.join().expect("both launches end");
    }

    let recalled = Recall::load(&home);
    let mut held = prompts(&recalled).to_vec();
    held.sort_unstable();
    let mut sent: Vec<String> = (0..2)
        .flat_map(|launch| (0..100).map(move |ask| format!("ask {launch}-{ask}")))
        .collect();
    sent.sort_unstable();
    assert_eq!(held, sent, "neither launch eats the other's lines");
    assert_eq!(
        written(&state)
            .lines()
            .filter(|line| serde_json::from_str::<serde_json::Value>(line).is_ok())
            .count(),
        200,
        "and every line left in the file is one whole object"
    );
}
