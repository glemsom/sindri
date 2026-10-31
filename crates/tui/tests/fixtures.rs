//! The facts a frame is launched with are the shared fixture's: a test that
//! needs another branch or session says so and takes the rest from
//! `common::launch()`, so a field added to `Launch` is one edit and not one per
//! test file. What drifts is the copy — a test that writes the whole literal
//! down again — so a launch here names only the facts a test has a reason to
//! rename.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::path::{Path, PathBuf};

/// The launch facts a test may spell out: the ones a fixture renames. A launch
/// that names anything else has copied the shared literal instead of taking the
/// rest from it, whatever field `Launch` grows next.
const RENAMED: [&str; 4] = ["session", "temp", "branch", "workspace"];

/// Every test source but the two the walk leaves out: the shared fixture is the
/// one file that names the whole launch, and this one is the law rather than a
/// fixture, whose own samples are launches.
fn sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let left_out = [root.join("common/mod.rs"), root.join("fixtures.rs")];
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the test tree") {
            let path = entry.expect("a test entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") && !left_out.contains(&path) {
                files.push(path);
            }
        }
    }
    files
}

/// The `Launch { … }` literals in a piece of Rust, whole: the scan reads a
/// launch and nothing else, so a `use` path and another struct's field of the
/// same name are not one of its facts.
fn launches(source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some(at) = lines[i].find("Launch {") else {
            i += 1;
            continue;
        };
        let mut depth = 0i32;
        let mut block = String::new();
        let mut j = i;
        while j < lines.len() {
            let line = lines[j];
            let from = if j == i { at + "Launch ".len() } else { 0 };
            let piece = line.get(from..).unwrap_or_default();
            block.push_str(piece);
            block.push('\n');
            depth += piece.matches('{').count() as i32 - piece.matches('}').count() as i32;
            j += 1;
            if depth <= 0 {
                break;
            }
        }
        blocks.push(block);
        i = j;
    }
    blocks
}

/// The fields a launch literal names before its `..` rest, which is where the
/// shared fixture takes over.
fn facts(block: &str) -> Vec<&str> {
    block
        .lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with(".."))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, _)| name)
        .filter(|name| !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect()
}

/// The facts a piece of Rust copies into a launch of its own, rather than taking
/// them from the shared fixture.
fn copied(source: &str) -> Vec<String> {
    launches(source)
        .iter()
        .flat_map(|block| facts(block))
        .filter(|fact| !RENAMED.contains(fact))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_test_names_only_the_launch_facts_it_renames() {
    let files = sources();
    assert!(files.len() > 20, "the test tree was found: {files:?}");
    let mut copies = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(&file).expect("a test source");
        for fact in copied(&source) {
            copies.push(format!("{} names `{fact}:`", file.display()));
        }
    }
    assert!(
        copies.is_empty(),
        "a test copied a launch fact instead of taking it from `common::launch()`: {copies:#?}"
    );
}

#[test]
fn the_scan_reads_a_launch_whole_and_takes_no_use_path_for_one() {
    let copy = "fn app() -> App {\n    App::new(\n        Launch {\n            branch: Some(\"main\".into()),\n            sandbox: \"bwrap\".to_string(),\n            max_turns: 250,\n        },\n        None,\n    )\n}\n";
    assert_eq!(copied(copy), ["sandbox", "max_turns"]);
    // A fact named outside a launch — a module path, another struct's field — is
    // not a copy of the fixture.
    let elsewhere = "use sindri_core::sandbox::Enforcement;\nlet report = Report { max_turns: 3, ..Default::default() };\n";
    assert!(copied(elsewhere).is_empty());
    // The shared fixture's own literal is read whole, or a copy could hide
    // behind a literal the scan could not see into.
    let shared =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/common/mod.rs"))
            .expect("the shared fixture");
    let launch = launches(&shared).remove(0);
    let names = facts(&launch);
    assert!(
        names.len() >= 9,
        "the shared launch is read whole: {names:?}"
    );
}
