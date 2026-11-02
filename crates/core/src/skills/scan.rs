//! One scope's directory, and the one level of it that counts: a skill is a
//! directory holding a `SKILL.md`, links to directories are followed because
//! that is how people share them, and a link to nothing is said out loud.

use std::path::{Path, PathBuf};

use super::catalog::{Catalog, Refusal, Scope, Skill};
use super::frontmatter;

const SKILL: &str = "SKILL.md";

/// Where one scope lives, from the directory it is relative to.
pub fn scope_dir(from: &Path) -> PathBuf {
    from.join(".agents").join("skills")
}

/// Everything one scope holds: its skills, and every file the scan refused. A
/// scope directory that is not there holds nothing, which is not a refusal.
pub fn scan(dir: &Path, scope: Scope) -> Catalog {
    let mut catalog = Catalog::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return catalog,
        Err(error) => {
            catalog.refused.push(Refusal {
                path: dir.to_path_buf(),
                line: None,
                reason: format!("the skills directory cannot be read: {error}"),
            });
            return catalog;
        }
    };

    let mut found: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    found.sort();
    for path in found {
        if !path.is_dir() {
            if let Some(refusal) = broken_link(&path) {
                catalog.refused.push(refusal);
            }
            continue;
        }
        let file = path.join(SKILL);
        if !file.is_file() {
            // A directory that is not a skill is not a refusal — but a
            // `SKILL.md` that is a link to nothing is one that was meant to be
            // there, and silence would hide exactly that.
            if let Some(refusal) = broken_link(&file) {
                catalog.refused.push(refusal);
            }
            continue;
        }
        match read_skill(&path, scope) {
            Ok(skill) => catalog.skills.push(skill),
            Err(refusal) => catalog.refused.push(refusal),
        }
    }
    catalog
}

/// One skill directory: its `SKILL.md` read, or the reason it was refused.
fn read_skill(dir: &Path, scope: Scope) -> Result<Skill, Refusal> {
    let file = dir.join(SKILL);
    let text = std::fs::read_to_string(&file).map_err(|error| Refusal {
        path: file.clone(),
        line: None,
        reason: format!("the file cannot be read: {error}"),
    })?;
    let read = frontmatter::parse(&text).map_err(|problem| Refusal {
        path: file.clone(),
        line: Some(problem.line),
        reason: problem.reason,
    })?;
    Ok(Skill {
        name: read.name,
        description: read.description,
        scope,
        model_invocable: read.model_invocable,
        user_invocable: read.user_invocable,
        dir: dir.to_path_buf(),
    })
}

/// A link whose target is gone. It is a diagnostic and not a failure: a
/// collection shared by symlink loses a target far more often than it loses
/// its owner's interest, and the rest of the catalog is still worth reading.
fn broken_link(path: &Path) -> Option<Refusal> {
    let link = std::fs::symlink_metadata(path).ok()?;
    if !link.file_type().is_symlink() {
        return None;
    }
    let target = std::fs::read_link(path).unwrap_or_default();
    // The link is named together with the way it fails, so a loop and a target
    // that is simply gone are not said in the same words.
    let why = match std::fs::metadata(path) {
        Ok(_) => return None,
        Err(error) => error,
    };
    Some(Refusal {
        path: path.to_path_buf(),
        line: None,
        reason: format!("a broken link to {}: {why}", target.display()),
    })
}
