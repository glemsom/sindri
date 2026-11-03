//! The tree a run stands in: the path the sandbox mounts, the repository root a
//! run's own files are read from, and the git facts a record and a rail both
//! want. One probe per fact, so a record cannot name a branch the frame never
//! showed — and one probe for the two facts boot pays for, so the root a run's
//! files are found from is the root the facts were read at, and no third
//! process is spent learning it twice.

use std::path::{Path, PathBuf};
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

/// What one probe of the tree says: the facts a record and a rail read, and the
/// root a run's own files are read from. The two arrive together because boot
/// pays for them together, and a launch that reads one of them has already paid
/// for the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub git: Option<Git>,
    /// The repository `dir` stands in, as git names its top level, or `None`
    /// when it is no repository. A repository above `dir` belongs to whoever
    /// put the checkout there, and is not walked past.
    pub root: Option<PathBuf>,
}

/// The root `dir` stands in, and the git facts read at it. A directory that is
/// not a repository, a repository with no commit yet, and a host without git
/// all answer `None`.
///
/// Two processes and no more, because this is boot's critical path: `rev-parse
/// --show-toplevel` names the root, and `status --porcelain=v2 --branch` reports
/// the branch, the commit and the dirt together. A repository is asked for its
/// root once, and every reader of the tree hangs off that one answer.
pub fn probe(dir: &Path) -> Probe {
    let Some(root) = root(dir) else {
        return Probe {
            git: None,
            root: None,
        };
    };
    let git = field(dir, &["status", "--porcelain=v2", "--branch"]).and_then(|status| {
        Some(Git {
            branch: head(&status)?,
            commit: oid(&status)?,
            dirty: dirty(&status),
        })
    });
    Probe {
        git,
        root: Some(root),
    }
}

/// The repository `dir` stands in, as git names its top level, or `None` when
/// it is no repository: the one root a run reads a repository's own files from.
pub fn root(dir: &Path) -> Option<PathBuf> {
    field(dir, &["rev-parse", "--show-toplevel"]).map(|line| top(&line))
}

/// git's top level as a path: canonical, because git reports the tree it really
/// walked and every reader of it compares paths.
fn top(top: &str) -> PathBuf {
    let path = PathBuf::from(top);
    path.canonicalize().unwrap_or(path)
}

/// The branch in the status's `# branch.head` header: the name git writes there,
/// but for a HEAD with no branch, which porcelain calls `(detached)` and a run
/// has always recorded as `HEAD`.
fn head(status: &str) -> Option<String> {
    let name = status
        .lines()
        .find_map(|line| line.strip_prefix("# branch.head "))?;
    Some(if name == "(detached)" { "HEAD" } else { name }.to_string())
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
