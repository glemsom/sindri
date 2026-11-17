//! The files of a workspace, walked as `git` reads them: the one walk the `@` screen's rows and the
//! draft's count are read off. The walk is the `ignore` crate's with `hidden(false)` and
//! `require_git(false)` turned around.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A directory in a working tree and a file in a worktree: a checkout's own bookkeeping and
/// never something a reader points a prompt at.
const GIT_DIRECTORY: &str = ".git";

/// The files of a workspace, relative to its root and sorted, and the root they stand under.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Files {
    root: Option<PathBuf>,
    paths: OnceLock<Vec<String>>,
}

impl Files {
    /// The files of `root`, walked now.
    pub fn walk(root: &Path) -> Files {
        Files {
            root: Some(root.to_path_buf()),
            paths: OnceLock::from(walked(root)),
        }
    }

    /// The files of `root`, walked the first time one is asked for.
    pub fn of(root: Option<&Path>) -> Files {
        Files {
            root: root.map(Path::to_path_buf),
            paths: OnceLock::new(),
        }
    }

    /// The root the files stand under, and none for a value holding no workspace.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// The workspace's files, the lazy walk happening here.
    pub fn paths(&self) -> &[String] {
        match self.root.as_deref() {
            Some(root) => self.paths.get_or_init(|| walked(root)),
            None => &[],
        }
    }

    /// Whether `word` is one of the paths, exactly.
    pub fn names(&self, word: &str) -> bool {
        self.paths().iter().any(|path| path == word)
    }
}

/// The walk itself: every file under `root`, as a path relative to it.
fn walked(root: &Path) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    let mut walk = ignore::WalkBuilder::new(root);
    walk.hidden(false).require_git(false);
    for entry in walk.build().flatten() {
        let path = entry.path();
        if entry.file_type().is_none_or(|kind| kind.is_dir()) {
            continue;
        }
        let Some(relative) = path.strip_prefix(root).ok().and_then(|rest| rest.to_str()) else {
            continue;
        };
        if relative.split('/').any(|step| step == GIT_DIRECTORY) {
            continue;
        }
        files.push(relative.to_owned());
    }
    // The walk hands siblings back in whatever order the filesystem keeps them, and the rank keeps
    // the order it is handed, so the files are sorted here and nowhere else.
    files.sort_unstable();
    files
}

#[cfg(test)]
mod tests;
