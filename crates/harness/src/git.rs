//! A repository a test builds, read by git with none of the developer's own
//! configuration: no global file and no system file, so a machine that signs its
//! commits, or carries a hooks directory, a commit template or a global excludes
//! file, decides nothing about whether a test passes. The identity is the
//! fixture's too, because a machine with none of its own still has to commit.

use std::path::Path;

/// The identity a fixture commits as: `-c` overrides rather than the
/// configuration git would otherwise be read with, so a commit is the fixture's
/// and never the machine's.
const IDENTITY: [&str; 4] = [
    "-c",
    "user.name=sindri",
    "-c",
    "user.email=sindri@example.invalid",
];

/// What every command here is read with: an empty global file and an empty
/// system file, so `commit.gpgsign`, a global hooks directory, a commit template
/// and a global excludes file all come from nowhere.
const ISOLATION: [(&str, &str); 2] = [
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_SYSTEM", "/dev/null"),
];

/// A repository git will answer for, on `main`, with nothing committed: the root
/// every reader of the tree stops at, and no facts to record yet.
pub fn init(dir: &Path) {
    init_on(dir, "main");
}

/// The same on a branch of the test's own name, where the branch is a fact the
/// test reads.
pub fn init_on(dir: &Path, branch: &str) {
    if let Err(error) = try_init_on(dir, branch) {
        panic!("{error}");
    }
}

/// The same again, reporting why instead of stopping: an eval is a process, and
/// its failures are words rather than a panic.
pub fn try_init(dir: &Path) -> Result<(), String> {
    try_init_on(dir, "main")
}

/// One `git` command in `dir`, its trimmed stdout: the isolation and the
/// identity are already on it, so a test asks for what it wants and never for
/// how git should be configured.
pub fn run(dir: &Path, args: &[&str]) -> String {
    let out = command(dir).args(args).output().expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Everything in the tree staged as one commit, and the commit it made. One
/// commit whatever the tree holds: a fixture that asks for a commit is asking
/// for the branch and the commit a probe names.
pub fn commit(dir: &Path, message: &str) -> String {
    run(dir, &["add", "--all"]);
    run(dir, &["commit", "-q", "--allow-empty", "-m", message]);
    run(dir, &["rev-parse", "HEAD"])
}

/// A git invocation in `dir` that reads no configuration of the developer's own.
fn command(dir: &Path) -> std::process::Command {
    let mut command = std::process::Command::new("git");
    command.args(IDENTITY).current_dir(dir).envs(ISOLATION);
    command
}

fn try_init_on(dir: &Path, branch: &str) -> Result<(), String> {
    let branch = format!("--initial-branch={branch}");
    let out = command(dir)
        .args(["init", "-q", &branch])
        .output()
        .map_err(|error| format!("git init: {error}"))?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!(
        "git init: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}
