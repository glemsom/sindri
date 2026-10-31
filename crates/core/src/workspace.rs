//! The tree a run stands in: the path the sandbox mounts, and the git facts a
//! record and a rail both want. One probe, so a record cannot name a branch the
//! frame never showed.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// The git facts of one workspace, every one of them read. A run in a
/// directory that is not a repository records no block at all, because silence
/// is honest and `clean` is a claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Git {
    pub branch: String,
    pub commit: String,
    /// Set from the status that was actually read: a `git` that cannot run
    /// leaves no block, rather than a tree called clean.
    pub dirty: bool,
}

/// The repository `dir` is inside of, if any. A directory that is not one, a
/// repository with no commit yet, and a host without git all answer `None`.
pub fn git(dir: &Path) -> Option<Git> {
    let branch = field(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let commit = field(dir, &["rev-parse", "HEAD"])?;
    let status = field(dir, &["status", "--porcelain"])?;
    Some(Git {
        branch,
        commit,
        dirty: !status.is_empty(),
    })
}

fn field(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
