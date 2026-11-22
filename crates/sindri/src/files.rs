use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const GIT_DIRECTORY: &str = ".git";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Files {
    root: Option<PathBuf>,
    paths: OnceLock<Vec<String>>,
}

impl Files {
    pub fn walk(root: &Path) -> Files {
        Files {
            root: Some(root.to_path_buf()),
            paths: OnceLock::from(walked(root)),
        }
    }

    pub fn of(root: Option<&Path>) -> Files {
        Files {
            root: root.map(Path::to_path_buf),
            paths: OnceLock::new(),
        }
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn paths(&self) -> &[String] {
        match self.root.as_deref() {
            Some(root) => self.paths.get_or_init(|| walked(root)),
            None => &[],
        }
    }

    pub fn names(&self, word: &str) -> bool {
        self.paths()
            .binary_search_by(|path| path.as_str().cmp(word))
            .is_ok()
    }
}

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
