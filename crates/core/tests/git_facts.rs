//! The git facts one probe reads — branch, commit and dirtiness — proven at
//! `workspace::probe`, the seam a record and the rail both hang off. Boot pays
//! for these on every launch, so how many processes they cost is part of the
//! spec.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a probe that cannot read its tree reports itself"
)]

mod support;

use std::path::{Path, PathBuf};

use sindri_core::instructions::Taken;
use sindri_core::session::{Env, Session};
use sindri_core::workspace::{Git, probe};
use support::config;
use tempfile::TempDir;

/// A repository with one commit, and the commit it planted.
fn repository() -> (TempDir, String) {
    let root = TempDir::new().expect("host");
    let head = support::commit(root.path());
    (root, head)
}

/// One `git` invocation in `dir`, its trimmed stdout.
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
fn a_repository_records_its_branch_commit_and_an_untouched_tree() {
    let (root, head) = repository();

    let facts = probe(root.path())
        .git
        .expect("a repository is a repository");

    assert_eq!(
        facts.branch, "main",
        "the branch as git names it: {facts:?}"
    );
    assert_eq!(facts.commit, head, "and the commit it is on");
    assert!(!facts.dirty, "a tree nothing touched is not dirty");
}

#[test]
fn an_untracked_file_makes_the_tree_dirty() {
    let (root, _) = repository();
    std::fs::write(root.path().join("uncommitted"), "wip\n").expect("an untracked file");

    let facts = probe(root.path())
        .git
        .expect("a repository is a repository");

    assert!(facts.dirty, "the status was read, so it is said: {facts:?}");
}

#[test]
fn a_detached_head_records_the_branch_named_head() {
    let (root, head) = repository();
    run(root.path(), &["checkout", "-q", "--detach"]);

    let facts = probe(root.path())
        .git
        .expect("a detached HEAD is still a repository");

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

    assert_eq!(
        probe(root.path()).git,
        None,
        "an unborn branch has no facts yet"
    );
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

    let facts = probe(root.path()).git;

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

/// A skill directory under `at`, holding a `SKILL.md` a scan will load.
fn skill(at: &Path, name: &str) {
    let dir = at.join(name);
    std::fs::create_dir_all(&dir).expect("a skill directory");
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: where {name} came from\n---\n\nA body.\n"),
    )
    .expect("SKILL.md");
}

/// Boot asks the tree where its repository root is once and hands the answer to
/// both readers, so a workspace three directories deep pays one `--show-toplevel`
/// rather than one per reader — and the rules and both scopes still arrive.
#[test]
fn a_launch_probes_the_repository_root_once_for_the_rules_and_the_skills() {
    let (root, _) = repository();
    std::fs::write(root.path().join("AGENTS.md"), "# House rules\n").expect("the root's rules");
    skill(&root.path().join(".agents/skills"), "root-scope-probe");
    let deep = root.path().join("crates/deep/inner");
    std::fs::create_dir_all(&deep).expect("a workspace three directories in");
    skill(&deep.join(".agents/skills"), "workspace-scope-probe");
    let home = TempDir::new().expect("a home");
    skill(&home.path().join(".agents/skills"), "user-scope-probe");

    let (shim, log) = counting_git();
    let path = format!(
        "{}:{}",
        shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    // SAFETY: no other thread of this process reads either variable while the
    // launch runs, and the shim and the home exist only for this test.
    unsafe {
        std::env::set_var("PATH", path);
        std::env::set_var("HOME", home.path());
    }
    let env = Env {
        workspace: deep.canonicalize().expect("a canonical workspace"),
        search_path: "/usr/bin:/bin".to_string(),
        state: root.path().join("state"),
    };

    let session = Session::open(&env, &config(&[])).expect("a session opens");

    let calls: Vec<String> = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let roots = calls
        .iter()
        .filter(|call| call.contains("--show-toplevel"))
        .count();
    assert_eq!(
        roots, 1,
        "the root is asked for once, and this launch asked {roots} times: {calls:?}"
    );
    assert!(
        calls.len() <= 2,
        "the probe's two processes are a launch's whole git bill, and this one paid {}: {calls:?}",
        calls.len()
    );
    let Taken::Found(found) = session.instructions() else {
        panic!(
            "the root's rules are the run's: {:?}",
            session.instructions()
        );
    };
    assert_eq!(
        found.path,
        root.path()
            .join("AGENTS.md")
            .canonicalize()
            .expect("the file"),
        "the rules are read from the root the same probe named"
    );
    // Each skill is planted in exactly one of the three scopes' directories and
    // named for it, so the names a human may reach for say which scope the one
    // probe reached. The binary's own skills are in every catalog and are not
    // this test's, hence the suffix.
    let offered = session.hand().names();
    let probes: Vec<&str> = offered
        .iter()
        .map(String::as_str)
        .filter(|name| name.ends_with("-probe"))
        .collect();
    assert_eq!(
        probes,
        vec![
            "root-scope-probe",
            "user-scope-probe",
            "workspace-scope-probe",
        ],
        "the workspace's own skills, the root's above it, and the user's: {offered:?}"
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
