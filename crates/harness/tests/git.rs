//! A repository the fixture builds is read by git with none of the developer's
//! own configuration: a machine whose git signs its commits decides nothing
//! here, and the fixture's commit lands anyway.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

use std::path::Path;

use sindri_harness::git::{commit, init, run};
use tempfile::TempDir;

/// A home whose `.gitconfig` signs every commit with a program that always
/// fails, so a git that reads it writes no commit object at all.
fn signing_home() -> TempDir {
    let home = TempDir::new().expect("a home");
    std::fs::write(
        home.path().join(".gitconfig"),
        "[commit]\n\tgpgsign = true\n[gpg]\n\tprogram = /bin/false\n",
    )
    .expect("a .gitconfig that refuses a signed commit");
    home
}

/// A repository and a commit made by a git that reads the developer's own
/// configuration — what the fixture's repository must not be, and so the one
/// identity and `-c` overrides it spells for itself: the fixture's come with
/// its isolation.
fn unisolated_commit(dir: &Path) -> std::process::Output {
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=sindri",
                "-c",
                "user.email=sindri@example.invalid",
            ])
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs")
    };
    let init = git(&["init", "-q", "--initial-branch=main"]);
    assert!(init.status.success(), "git init: {init:?}");
    std::fs::write(dir.join("readme"), "one line\n").expect("a file to commit");
    let add = git(&["add", "--all"]);
    assert!(add.status.success(), "git add: {add:?}");
    git(&["commit", "-q", "-m", "the first commit"])
}

#[test]
fn a_repository_the_fixture_builds_commits_under_a_gitconfig_that_would_refuse_a_signed_one() {
    let home = signing_home();
    // SAFETY: a harness test owns its process, so nothing else reads the
    // environment while these are set: the `git` the fixture spawns is the child
    // that must hear them.
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::remove_var("GIT_CONFIG_GLOBAL");
        std::env::remove_var("GIT_CONFIG_SYSTEM");
    }

    // The `.gitconfig` is one a plain git refuses every commit under: signing
    // is on and the program it signs with always fails, so no commit object is
    // written at all.
    let bare = TempDir::new().expect("host");
    let refused = unisolated_commit(bare.path());
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success() && said.contains("gpg failed to sign"),
        "the .gitconfig refuses an unisolated commit: {refused:?}"
    );

    let root = TempDir::new().expect("host");
    init(root.path());
    std::fs::write(root.path().join("readme"), "one line\n").expect("a file to commit");
    let head = commit(root.path(), "the first commit");

    assert_eq!(
        run(root.path(), &["cat-file", "-t", &head]),
        "commit",
        "the commit the fixture reports is a commit object"
    );
    assert_eq!(
        run(root.path(), &["rev-parse", "--abbrev-ref", "HEAD"]),
        "main",
        "one branch"
    );
    assert_eq!(
        run(root.path(), &["rev-list", "--count", "HEAD"]),
        "1",
        "one commit"
    );
    assert_eq!(
        run(root.path(), &["status", "--porcelain"]),
        "",
        "one clean answer"
    );
}
