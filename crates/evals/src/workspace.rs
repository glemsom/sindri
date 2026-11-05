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

/// The file a repository's rules live in, at its root: eval 10's copy carries
/// one, and the other nine stand in a repository that says nothing.
pub const AGENTS: &str = "AGENTS.md";

pub const SENTINEL_TEXT: &str = "the run's own file\n";

/// Where under the eval's config directory the run's file sits.
const SETTINGS: &str = "sindri";

/// The one key an eval's config carries: no extras at all, whatever home
/// directory holds a `.cache` or a `.cargo` this suite never asked for.
const PINNED_CONFIG: &str = "extra_writable_paths = []\n";

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
        // The repo skill scope is walked from the repository root and from
        // nowhere else, so a copy that is not a repository has no catalog at all
        // and eval 9 would have no skill to trigger. The fixture's `git init`
        // leaves an unborn branch, which is no commit: the record still carries
        // no git block, exactly as the fixture carried none before.
        sindri_harness::git::try_init(&repo)
            .map_err(|error| format!("the fixture copy: {error}"))?;
        let state = root.path().join("state");
        let config = root.path().join("config");
        let settings = config.join(SETTINGS);
        for dir in [&state, &config, &settings] {
            fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        }
        // The suite's sandbox is the suite's: a writable set that fell back to
        // the shipped floor would mount whichever home directory ran the eval,
        // and eval 7 would then be probing a sandbox nobody described here. An
        // explicit empty list is a list, so it wins over the floor.
        let file = settings.join("config.toml");
        fs::write(&file, PINNED_CONFIG).map_err(|error| format!("{}: {error}", file.display()))?;
        fs::write(repo.join(SENTINEL), SENTINEL_TEXT)
            .map_err(|error| format!("write sentinel: {error}"))?;
        Ok(Workspace {
            root,
            repo,
            state,
            config,
        })
    }

    /// Gives the copy the `AGENTS.md` the case's repository carries, read from
    /// the fixture rather than written here: the rules a case turns on are a
    /// fixture, and the run reads them through this one file like any other.
    pub fn install_instructions(&self, instructions: Option<&str>) -> Result<(), String> {
        let Some(name) = instructions else {
            return Ok(());
        };
        let from = Path::new(FIXTURES).join(name);
        fs::copy(&from, self.repo.join(AGENTS))
            .map_err(|error| format!("{}: {error}", from.display()))?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        self.root.path()
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
