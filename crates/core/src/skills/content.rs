//! One skill as a `skill` call returns it: the body with its frontmatter
//! stripped, and the files beside it, so a relative reference in the body
//! resolves. A scanned skill's body and listing are read from disk at the
//! moment of the call, because a file that changed under the run is the file
//! the model is about to follow; a built-in's are the bytes the binary already
//! holds, and no directory is named for them because there is none.

use std::path::Path;

use super::SKILL;
use super::catalog::{Content, File, Skill};
use super::frontmatter;

/// How far a listing follows a directory. A skill is a body and the files it
/// names; a walk that follows every level is a walk of somebody's whole tree.
const DEPTH: usize = 3;

impl Skill {
    /// The body under the fence. A file that vanished, lost its fence or holds
    /// nothing under it is a refusal naming where the body was read from — the
    /// model is told what went wrong rather than handed an empty instruction.
    pub fn body(&self) -> Result<String, String> {
        let (text, at) = match &self.content {
            Content::Directory(dir) => {
                let path = dir.join(SKILL);
                let text = std::fs::read_to_string(&path)
                    .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
                (text, path.display().to_string())
            }
            Content::Compiled { source, .. } => (
                (*source).to_string(),
                format!("the built-in skill `{}`", self.name),
            ),
        };
        match frontmatter::body(&text) {
            Some(body) if !body.trim().is_empty() => Ok(body.trim().to_string()),
            Some(_) => Err(format!("{at} holds nothing under its frontmatter")),
            None => Err(format!(
                "{at} no longer opens with a frontmatter fence that closes"
            )),
        }
    }

    /// Every file the body may name, relative to the skill and sorted, a
    /// directory ending in `/`: what `references/api.md` in a body means is this
    /// list's `references/api.md`. Dot-files are left out — they are the
    /// editor's and the platform's, never a reference a body makes.
    pub fn listing(&self) -> Vec<String> {
        match &self.content {
            Content::Directory(dir) => {
                let mut found = Vec::new();
                walk(dir, "", DEPTH, &mut found);
                found.sort();
                found
            }
            Content::Compiled { files, .. } => {
                let mut found: Vec<String> =
                    files.iter().map(|file| file.path.to_string()).collect();
                found.sort();
                found
            }
        }
    }

    /// The files that come with the body rather than sitting in a directory a
    /// model opens: a built-in's, compiled in. A scanned skill answers none —
    /// its files are in [`Skill::dir`], where the model reads them.
    pub fn files(&self) -> &'static [File] {
        match &self.content {
            Content::Directory(_) => &[],
            Content::Compiled { files, .. } => files,
        }
    }

    /// The directory the body and its files are read from, where there is one.
    /// A built-in has none, and a result that named one would be pointing a
    /// model at a path that does not exist.
    pub fn dir(&self) -> Option<&Path> {
        match &self.content {
            Content::Directory(dir) => Some(dir),
            Content::Compiled { .. } => None,
        }
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
