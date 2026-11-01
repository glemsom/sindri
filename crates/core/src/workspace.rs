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
///
/// Two processes and no more, because this is boot's critical path: `rev-parse
/// --abbrev-ref` is the one call that names a detached HEAD `HEAD`, and
/// `status --porcelain=v2 --branch` reports the commit and the dirt together.
pub fn git(dir: &Path) -> Option<Git> {
    let branch = field(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let status = field(dir, &["status", "--porcelain=v2", "--branch"])?;
    Some(Git {
        branch,
        commit: oid(&status)?,
        dirty: dirty(&status),
    })
}

/// The commit in the status's `# branch.oid` header, absent while the branch is
/// unborn — the `(initial)` git writes there is not a commit.
fn oid(status: &str) -> Option<String> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("# branch.oid "))
        .filter(|oid| *oid != "(initial)")
        .map(str::to_string)
}

/// Every line of a `--porcelain=v2` report that is not a `#` header is an entry.
fn dirty(status: &str) -> bool {
    status
        .lines()
        .any(|line| !line.is_empty() && !line.starts_with('#'))
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
