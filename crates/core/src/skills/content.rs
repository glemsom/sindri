//! One skill as a `skill` call returns it: the body with its frontmatter
//! stripped, and the directory it lives in with the files beside the body, so a
//! relative reference in the body resolves. Both are read from disk at the
//! moment of the call, because a file that changed under the run is the file
//! the model is about to follow.

use std::path::Path;

use super::SKILL;
use super::catalog::Skill;
use super::frontmatter;

/// How far a listing follows a directory. A skill is a body and the files it
/// names; a walk that follows every level is a walk of somebody's whole tree.
const DEPTH: usize = 3;

impl Skill {
    /// The body under the fence. A file that vanished, lost its fence or holds
    /// nothing under it is a refusal with a path in it — the model is told what
    /// went wrong rather than handed an empty instruction.
    pub fn body(&self) -> Result<String, String> {
        let path = self.dir.join(SKILL);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
        match frontmatter::body(&text) {
            Some(body) if !body.trim().is_empty() => Ok(body.trim().to_string()),
            Some(_) => Err(format!(
                "{} holds nothing under its frontmatter",
                path.display()
            )),
            None => Err(format!(
                "{} no longer opens with a frontmatter fence that closes",
                path.display()
            )),
        }
    }

    /// Every file under the skill directory, relative to it and sorted, a
    /// directory ending in `/`: what `references/api.md` in a body means is this
    /// list's `references/api.md`. Dot-files are left out — they are the
    /// editor's and the platform's, never a reference a body makes.
    pub fn listing(&self) -> Vec<String> {
        let mut found = Vec::new();
        walk(&self.dir, "", DEPTH, &mut found);
        found.sort();
        found
    }
}

fn walk(dir: &Path, prefix: &str, depth: usize, found: &mut Vec<String>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let shown = format!("{prefix}{name}");
        if path.is_dir() {
            found.push(format!("{shown}/"));
            walk(&path, &format!("{shown}/"), depth - 1, found);
        } else {
            found.push(shown);
        }
    }
}
