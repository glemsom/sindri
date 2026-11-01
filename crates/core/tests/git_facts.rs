//! The git facts one probe reads — branch, commit and dirtiness — proven at
//! `workspace::git`, the seam a record and the rail both hang off. Boot pays for
//! these on every launch, so how many processes they cost is part of the spec.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a probe that cannot read its tree reports itself"
)]

mod support;

use std::path::{Path, PathBuf};

use sindri_core::workspace::{Git, git};
use tempfile::TempDir;

/// A repository with one commit, and the commit it planted.
fn repository() -> (TempDir, String) {
    let root = TempDir::new().expect("host");
    let head = support::commit(root.path());
    (root, head)
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn a_repository_records_its_branch_its_commit_and_a_tree_nothing_touched() {
    let (root, head) = repository();

    let facts = git(root.path()).expect("a repository is a repository");

    assert_eq!(
        facts.branch, "main",
        "the branch as git names it: {facts:?}"
    );
    assert_eq!(facts.commit, head, "and the commit it is on");
    assert!(!facts.dirty, "a tree nothing touched is not dirty");
}

#[test]
fn an_untracked_file_makes_the_tree_dirty() {
    let (root, _head) = repository();
    std::fs::write(root.path().join("uncommitted"), "wip\n").expect("an untracked file");

    let facts = git(root.path()).expect("a repository is a repository");

    assert!(facts.dirty, "the status was read, so it is said: {facts:?}");
}

#[test]
fn a_detached_head_records_the_branch_named_head() {
    let (root, head) = repository();
    run(root.path(), &["checkout", "-q", "--detach"]);

    let facts = git(root.path()).expect("a detached HEAD is still a repository");

    assert_eq!(
        facts.branch, "HEAD",
        "detached is named, not hidden: {facts:?}"
    );
    assert_eq!(
        facts.commit, head,
        "and it names the commit it was detached at"
    );
}

/// A repository with nothing committed has no commit to record, so the probe
/// answers `None` rather than an empty block — the same answer a failing
/// `rev-parse HEAD` gave.
#[test]
fn a_repository_with_no_commit_records_no_git_block() {
    let root = TempDir::new().expect("host");
    run(root.path(), &["init", "-q", "--initial-branch=main"]);

    assert_eq!(git(root.path()), None, "an unborn branch has no facts yet");
}

/// Boot is a critical path, and `git status --porcelain=v2 --branch` already
/// knows the commit and the dirt: the probe must not pay a third fork for them.
#[test]
fn a_probe_reads_the_git_facts_in_two_processes() {
    let (root, head) = repository();
    let (shim, log) = counting_git();
    // Each nextest test owns its process, so a shimmed path cannot leak into
    // another test's probe.
    let path = format!(
        "{}:{}",
        shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    // SAFETY: no other thread of this process reads the environment while the
    // probe runs; the shimmed path exists only for this test.
    unsafe { std::env::set_var("PATH", path) };

    let facts = git(root.path());

    let calls: Vec<String> = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    assert!(
        calls.len() <= 2,
        "a launch pays the probe twice at most, and this one paid {}: {calls:?}",
        calls.len()
    );
    assert_eq!(
        facts.map(|Git { branch, commit, .. }| (branch, commit)),
        Some(("main".to_string(), head)),
        "and reads the same two facts out of them: {calls:?}"
    );
    assert!(
        calls.iter().any(|call| call.contains("--porcelain=v2")),
        "the status is asked for its branch and oid headers: {calls:?}"
    );
}

/// A directory holding a `git` that logs the arguments it was reached with and
/// then runs the real one, so a test can count the forks the probe makes.
fn counting_git() -> (PathBuf, PathBuf) {
    let root = TempDir::new().expect("host");
    let log = root.path().join("calls");
    let shim = root.path().join("git");
    let real = ["/usr/bin/git", "/usr/local/bin/git", "/bin/git"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .expect("a git on this host");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexec '{}' \"$@\"\n",
            log.display(),
            real.display()
        ),
    )
    .expect("the shim");
    let mode = std::os::unix::fs::PermissionsExt::from_mode(0o755);
    std::fs::set_permissions(&shim, mode).expect("an executable shim");
    (root.keep(), log)
}
