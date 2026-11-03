//! The extras: the writable paths beyond the workspace and the session temp,
//! and where they start. A run opens in a fresh checkout with nothing
//! configured and can still build, so the list has a shipped floor — and the
//! list stays the user's sentence, because a level that names one replaces the
//! floor wholesale and a list named empty means none.

use std::path::{Path, PathBuf};

/// The shipped floor, relative to the home directory. Each is a convenience a
/// toolchain keeps its caches in, and each yields to the host: one that is not
/// a directory here, or that nests with the workspace, is dropped without a
/// word rather than turning a fresh checkout into a usage error.
pub const FLOOR: [&str; 4] = [".cache", "go", ".cargo", ".rustup"];

/// Where a run's extras come from: the floor when no level named a list, and
/// exactly what was named when one did. The two are one value and not a list
/// plus a flag, so a blank setting can never quietly mean four mounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extras {
    /// No flag, variable or file named one, so the floor applies.
    Floor,
    /// A level named a list — an empty one included, which means none.
    Named(Vec<PathBuf>),
}

impl Extras {
    /// The paths to mount besides the workspace. A named list is returned as it
    /// was given, already canonical and checked by the config; the floor is
    /// resolved against this host's home directory and this run's workspace.
    pub fn resolve(&self, home: Option<&Path>, workspace: &Path) -> Vec<PathBuf> {
        match self {
            Extras::Named(paths) => paths.clone(),
            Extras::Floor => floor(home, workspace),
        }
    }
}

/// The floor as this host has it: a shipped path that is not a directory, that
/// would nest with the workspace, or that repeats one already taken is dropped
/// silently. The writable set's rules are the same, and a shipped path is a
/// convenience rather than a sentence, so it yields instead of refusing.
fn floor(home: Option<&Path>, workspace: &Path) -> Vec<PathBuf> {
    let Some(home) = home else {
        return Vec::new();
    };
    let mut kept: Vec<PathBuf> = Vec::new();
    for name in FLOOR {
        let Ok(path) = home.join(name).canonicalize() else {
            continue;
        };
        // `/` is never a mount, and neither is a file where a cache belongs.
        if !path.is_dir() || path.parent().is_none() {
            continue;
        }
        let nests = path.starts_with(workspace) || workspace.starts_with(&path);
        let repeats = kept
            .iter()
            .any(|seen| seen == &path || path.starts_with(seen) || seen.starts_with(&path));
        if nests || repeats {
            continue;
        }
        kept.push(path);
    }
    kept
}
