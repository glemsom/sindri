use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sindri_core::recall::{KEPT, LONGEST, Recall};
use tempfile::TempDir;

struct Ground {
    _root: TempDir,
    state: PathBuf,
}

impl Ground {
    fn new() -> Ground {
        let root = tempfile::tempdir().expect("a temp ground");
        let state = root.path().join("state");
        std::fs::create_dir_all(&state).expect("a state directory");
        Ground { _root: root, state }
    }

    fn environment(&self) -> BTreeMap<String, String> {
        [(
            String::from("XDG_STATE_HOME"),
            self.state.to_string_lossy().into_owned(),
        )]
        .into_iter()
        .collect()
    }

    fn file(&self) -> PathBuf {
        self.state.join("sindri").join("recall.jsonl")
    }

    fn loaded(&self) -> Recall {
        Recall::load(&self.environment())
    }
}

fn prompts(recall: &Recall) -> Vec<String> {
    recall
        .entries()
        .iter()
        .map(|entry| entry.prompt().to_owned())
        .collect()
}

fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("the recall file is readable")
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_prompt_sent_by_one_launch_is_the_next_launchs_to_recall() {
    let ground = Ground::new();
    let mut sent = ground.loaded();
    sent.send("why does the retry test flake?");

    let next = ground.loaded();
    assert_eq!(
        next.older(0),
        Some("why does the retry test flake?"),
        "the words one launch sent are the ones the next hands back"
    );
    assert_eq!(
        next.path(),
        Some(ground.file().as_path()),
        "kept in recall.jsonl beside the sessions in the state directory"
    );
}

#[test]
fn a_line_holds_an_at_and_the_prompt_and_nothing_else() {
    let ground = Ground::new();
    ground.loaded().send("say it again");

    let written = lines(&ground.file());
    assert_eq!(written.len(), 1, "one prompt is one line");
    let line: serde_json::Value = serde_json::from_str(&written[0]).expect("a line is JSON");
    let object = line.as_object().expect("a line is an object");
    assert_eq!(
        object.keys().collect::<Vec<_>>(),
        vec!["at", "prompt"],
        "an `at` and a `prompt`, and no workspace, model or outcome: {line}"
    );
    assert_eq!(object["prompt"], "say it again");
    assert!(
        object["at"].as_str().is_some_and(|at| at.ends_with('Z')),
        "the time is written the way the record writes one: {line}"
    );
}

#[test]
fn the_newest_prompt_is_kept_once_and_an_older_one_keeps_its_place() {
    let ground = Ground::new();
    let mut recall = ground.loaded();
    recall.send("the same words");
    recall.send("the same words");
    assert_eq!(
        prompts(&recall),
        vec!["the same words"],
        "a prompt sent again right after itself is one stroke to walk past, not two"
    );

    recall.send("something else");
    recall.send("the same words");
    assert_eq!(
        prompts(&recall),
        vec!["the same words", "something else", "the same words"],
        "and the same words said again later keep their own place in time"
    );
    assert_eq!(
        prompts(&ground.loaded()),
        vec!["the same words", "something else", "the same words"],
        "the file keeps them both, so the next launch walks the order things happened"
    );
}

#[test]
fn a_recall_past_its_cap_drops_the_oldest_and_rewrites_the_file() {
    let ground = Ground::new();
    let mut recall = ground.loaded();
    for spoken in 1..=KEPT + 5 {
        recall.send(&format!("prompt {spoken}"));
    }

    let loaded = ground.loaded();
    assert_eq!(
        prompts(&loaded).len(),
        KEPT,
        "a launch carries no more than the cap says"
    );
    assert_eq!(
        loaded.older(0),
        Some(format!("prompt {}", KEPT + 5).as_str()),
        "the newest is still the newest"
    );
    assert_eq!(
        loaded.older(KEPT - 1),
        Some("prompt 6"),
        "and the five oldest are the ones dropped"
    );
    assert_eq!(
        lines(&ground.file()).len(),
        KEPT,
        "the file over its cap is rewritten at load, and holds what the launch carries"
    );
}

#[test]
fn a_prompt_past_the_ceiling_is_not_kept() {
    let ground = Ground::new();
    let mut recall = ground.loaded();
    let log = "x".repeat(LONGEST + 1);
    recall.send(&log);
    recall.send("something a key is worth walking to");

    assert_eq!(
        prompts(&recall),
        vec!["something a key is worth walking to"],
        "a pasted log is not a stroke an arrow key should walk past"
    );
    assert_eq!(
        lines(&ground.file()).len(),
        1,
        "and it is not on disk either"
    );
}

#[test]
fn a_line_the_reader_broke_is_skipped_and_the_rest_are_kept() {
    let ground = Ground::new();
    let mut recall = ground.loaded();
    recall.send("the prompt that parses");
    let mut text = std::fs::read_to_string(ground.file()).expect("the file");
    text.insert_str(0, "not a line of JSON\n\n");
    std::fs::write(ground.file(), text).expect("the file takes it");

    let loaded = ground.loaded();
    assert_eq!(
        prompts(&loaded),
        vec!["the prompt that parses"],
        "a line a reader broke by hand is skipped, and the launch reads the rest"
    );
}

#[test]
fn a_recall_with_nowhere_to_write_still_recalls_what_it_sent() {
    let nowhere: BTreeMap<String, String> = BTreeMap::new();
    let mut recall = Recall::load(&nowhere);
    recall.send("said into the void");

    assert_eq!(
        recall.path(),
        None,
        "a machine with no state home has nowhere to keep the file"
    );
    assert_eq!(
        recall.older(0),
        Some("said into the void"),
        "and the launch still hands back what it sent"
    );
}

#[test]
fn the_file_is_the_readers_alone() {
    let ground = Ground::new();
    ground.loaded().send("private words");

    let mode = std::fs::metadata(ground.file())
        .expect("the recall file is there")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "the reader's words are readable and writable by the reader and nobody else"
    );
}

#[test]
fn two_launches_appending_at_once_keep_each_others_lines() {
    let ground = Ground::new();
    let mut one = ground.loaded();
    let mut other = ground.loaded();
    one.send("from the first terminal");
    other.send("from the second terminal");

    let loaded = ground.loaded();
    let kept = prompts(&loaded);
    assert_eq!(
        kept,
        vec!["from the first terminal", "from the second terminal"],
        "an append is an append: neither launch's line ate the other's"
    );
}
