//! The trees these tests are written against: a repository with a workspace
//! three crates deep in it, and one skill directory holding one `SKILL.md`.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A repository with nothing committed and a workspace three crates deep inside
/// it, which is where a run in this monorepo really boots.
pub fn repo() -> (TempDir, PathBuf) {
    let root = TempDir::new().expect("a repository");
    init(root.path());
    let workspace = root.path().join("crates/core/src");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    (root, workspace)
}

/// A repository git will answer for, with nothing committed: the root a scan
/// stops at is the root git names, so the marker is a repository and not a
/// directory called `.git`.
pub fn init(dir: &Path) {
    let out = std::process::Command::new("git")
        .args(["init", "-q", "--initial-branch=main"])
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git init: {out:?}");
}

/// One skill directory holding the `SKILL.md` it is given.
pub fn skill(at: &Path, name: &str, body: &str) {
    let dir = at.join(name);
    std::fs::create_dir_all(&dir).expect("a skill directory");
    std::fs::write(dir.join("SKILL.md"), body).expect("a SKILL.md");
}
