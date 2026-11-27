use std::collections::BTreeMap;

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
    recall.entries().iter().map(Entry::prompt).collect()
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
        recall.entries()[0].at(),
        "2026-02-11T09:14:03Z",
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
fn no_state_directory_and_no_home_load_empty_and_write_nothing() {
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
fn a_recall_with_nowhere_to_write_still_holds_what_the_launch_sent() {
    let mut recall = Recall::none();

    recall.append("what a launch with no state home asked");

    assert_eq!(prompts(&recall), ["what a launch with no state home asked"]);
}
