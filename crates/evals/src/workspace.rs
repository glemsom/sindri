//! One eval's isolated host: a copy of the fixture repository to work in, the
//! state and config directories a user would have, and the sentinel file the
//! sandbox eval must find untouched.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

pub const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures");

/// A file every eval's copy starts with, so an escape that rewrote the run's own
/// files is visible afterwards.
pub const SENTINEL: &str = "sentinel.txt";

pub const SENTINEL_TEXT: &str = "the run's own file\n";

pub struct Workspace {
    root: TempDir,
    pub repo: PathBuf,
    pub state: PathBuf,
    pub config: PathBuf,
}

impl Workspace {
    pub fn new() -> Result<Self, String> {
        let root = TempDir::new().map_err(|error| format!("eval temp dir: {error}"))?;
        let repo = root.path().join("repo");
        copy_dir(Path::new(FIXTURES).join("repo").as_path(), &repo)?;
        let state = root.path().join("state");
        let config = root.path().join("config");
        for dir in [&state, &config] {
            fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        }
        fs::write(repo.join(SENTINEL), SENTINEL_TEXT)
            .map_err(|error| format!("write sentinel: {error}"))?;
        Ok(Workspace {
            root,
            repo,
            state,
            config,
        })
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.repo.join(relative)
    }
}

pub fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|error| format!("{}: {error}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|error| format!("{}: {error}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", from.display()))?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("{}: {error}", target.display()))?;
        }
    }
    Ok(())
}

pub fn files_containing(dir: &Path, needle: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if fs::read_to_string(&path).is_ok_and(|text| text.contains(needle)) {
                found.push(
                    path.strip_prefix(dir)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }
    found.sort();
    found
}
