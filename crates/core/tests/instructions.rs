//! A repository's `AGENTS.md`, read once at boot: the file at the repository
//! root reaches the model, a repository that has none says nothing, and one past
//! the stated cap arrives cut with a marker rather than silently short.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken seam is reported"
)]

mod support;

use sindri_core::instructions::{CAP, Taken, read};
use tempfile::TempDir;

/// A workspace three directories deep in a checkout, and the root's
/// `AGENTS.md` as it was written.
fn checkout() -> (TempDir, std::path::PathBuf, &'static str) {
    const RULES: &str = "# House rules\n\nRun `make check` before you commit.\n";
    let root = TempDir::new().expect("host");
    support::commit(root.path());
    std::fs::write(root.path().join("AGENTS.md"), RULES).expect("the repository's rules");
    let deep = root.path().join("crates/deep/inner");
    std::fs::create_dir_all(&deep).expect("a workspace three directories in");
    (root, deep, RULES)
}

#[test]
fn a_workspace_three_directories_deep_finds_the_root() {
    let (root, deep, rules) = checkout();

    let taken = read(&deep);

    let Taken::Found(found) = &taken else {
        panic!("the root's AGENTS.md is the run's: {taken:?}");
    };
    assert_eq!(
        found.path,
        root.path()
            .join("AGENTS.md")
            .canonicalize()
            .expect("the file"),
        "the path is the repository root's, not the workspace's"
    );
    assert_eq!(found.text, rules, "and the text is the file, whole");
    assert_eq!(
        found.bytes,
        rules.len(),
        "the bytes handed over are counted"
    );
}

#[test]
fn a_directory_that_is_no_repository_finds_its_own() {
    let root = TempDir::new().expect("host");
    std::fs::write(root.path().join("AGENTS.md"), "one rule\n").expect("rules");

    let taken = read(root.path());

    assert!(
        matches!(&taken, Taken::Found(found) if found.text == "one rule\n"),
        "the workspace itself is where a non-repository's rules live: {taken:?}"
    );
}

#[test]
fn no_agents_md_adds_nothing_and_says_nothing() {
    let root = TempDir::new().expect("host");
    support::commit(root.path());

    let taken = read(root.path());

    assert_eq!(
        taken,
        Taken::Absent,
        "a repository with no rules is silence"
    );
    assert_eq!(taken.line(), None, "nothing is said about a file not there");
    assert_eq!(taken.block(), None, "and nothing is handed to the model");
}

#[test]
fn a_file_past_the_cap_is_cut_with_a_marker() {
    let root = TempDir::new().expect("host");
    // A rule per line, so the file is plainly past the cap and every line is
    // whole text rather than one unbroken word.
    let long = "a rule that goes on\n".repeat(CAP);
    std::fs::write(root.path().join("AGENTS.md"), &long).expect("rules");

    let taken = read(root.path());

    let Taken::Found(found) = &taken else {
        panic!("a file past the cap is still found: {taken:?}");
    };
    assert_eq!(found.bytes, CAP, "the cap is what was handed over");
    assert!(
        found.text.len() <= CAP + 256,
        "the cut text is the cap plus the marker, not {} bytes",
        found.text.len()
    );
    assert!(
        found.text.starts_with(&long[..CAP]),
        "what the model sees is the head of the file, byte for byte"
    );
    let marker = found
        .text
        .get(CAP..)
        .expect("the marker sits after the cut text");
    assert!(
        marker.contains("truncated") && marker.contains(&CAP.to_string()),
        "the cut is stated in the text the model receives, not silently dropped: {marker:?}"
    );
    assert!(
        taken
            .block()
            .is_some_and(|block| block.contains("truncated")),
        "and it survives into the text the model is handed"
    );
}

#[test]
fn an_unreadable_file_is_a_diagnostic_naming_reason() {
    let root = TempDir::new().expect("host");
    // A directory where the file belongs: unreadable as a file whatever the
    // uid the test runs as, which a permission bit is not.
    std::fs::create_dir(root.path().join("AGENTS.md")).expect("a directory in its place");

    let taken = read(root.path());

    let Taken::Unreadable { path, reason } = &taken else {
        panic!("a file that cannot be read is a diagnostic: {taken:?}");
    };
    assert_eq!(
        path,
        &root.path().join("AGENTS.md"),
        "the path is named, so a reader knows what to fix"
    );
    assert!(!reason.is_empty(), "so is the reason");
    let line = taken.line().expect("a diagnostic is said");
    assert!(
        line.contains("AGENTS.md") && line.contains(reason),
        "in one sentence naming both: {line}"
    );
    assert_eq!(taken.block(), None, "and nothing is handed to the model");
}

#[test]
fn a_nested_agents_md_is_never_read() {
    let (root, deep, _) = checkout();
    std::fs::remove_file(root.path().join("AGENTS.md")).expect("the root has none");
    std::fs::write(deep.join("AGENTS.md"), "a nested rule\n").expect("a file where the run stands");

    let taken = read(&deep);

    assert_eq!(
        taken,
        Taken::Absent,
        "the root's file is the repository's rules, and a nested one is not a second"
    );
}

#[test]
fn a_claude_md_is_no_fallback() {
    let root = TempDir::new().expect("host");
    support::commit(root.path());
    std::fs::write(root.path().join("CLAUDE.md"), "another tool's file\n").expect("a file");

    let taken = read(root.path());

    assert_eq!(taken, Taken::Absent, "and another tool's file is not ours");
}

#[test]
fn the_lead_in_above_the_rules_costs_one_short_line() {
    // A checkout three directories deep, so the line is measured with the whole
    // absolute path in it: the line is re-sent on every turn, and the path is
    // the one thing it may grow by.
    let (root, deep, rules) = checkout();

    let taken = read(&deep);
    let block = taken.block().expect("the rules are handed over");
    let (lead_in, text) = block.split_once("\n\n").expect("a lead-in, then the rules");

    assert_eq!(
        text,
        format!(
            "<project_instructions>\n{}\n</project_instructions>",
            rules.trim_end()
        ),
        "the file's words follow the tags whole"
    );
    let path = root
        .path()
        .join("AGENTS.md")
        .canonicalize()
        .expect("the file");
    assert!(
        lead_in.contains(&path.display().to_string()),
        "the block opens by naming the file's path: {lead_in:?}"
    );
    assert!(
        lead_in.starts_with("## Repository instructions"),
        "and heads the block with what it is: {lead_in:?}"
    );
    let words = lead_in.split_whitespace().count();
    assert!(
        words <= 15,
        "the lead-in is one short line, not {words} words: {lead_in:?}"
    );
}
