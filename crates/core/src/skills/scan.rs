//! One scope's directory, and the one level of it that counts: a skill is a
//! directory holding a `SKILL.md`, links to directories are followed because
//! that is how people share them, and a link to nothing is said out loud.

use std::path::{Path, PathBuf};

use super::SKILL;
use super::catalog::{Content, Found, Refusal, Scope, Skill, Where};
use super::frontmatter;

/// Where one scope lives, from the directory it is relative to.
pub fn scope_dir(from: &Path) -> PathBuf {
    from.join(".agents").join("skills")
}

/// Everything one scope holds: its skills, and every file the scan refused. A
/// scope directory that is not there holds nothing, which is not a refusal.
pub fn scan(dir: &Path, scope: Scope) -> Found {
    let mut found = Found::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return found,
        Err(error) => {
            found.refused.push(Refusal {
                at: Where::File(dir.to_path_buf()),
                line: None,
                reason: format!("the skills directory cannot be read: {error}"),
            });
            return found;
        }
    };

    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if !path.is_dir() {
            if let Some(refusal) = broken_link(&path) {
                found.refused.push(refusal);
            }
            continue;
        }
        let file = path.join(SKILL);
        if !file.is_file() {
            // A directory that is not a skill is not a refusal — but a
            // `SKILL.md` that is a link to nothing is one that was meant to be
            // there, and silence would hide exactly that.
            if let Some(refusal) = broken_link(&file) {
                found.refused.push(refusal);
            }
            continue;
        }
        match read_skill(&path, scope) {
            Ok(skill) => found.skills.push(skill),
            Err(refusal) => found.refused.push(refusal),
        }
    }
    found
}

/// One skill directory: its `SKILL.md` read, or the reason it was refused.
fn read_skill(dir: &Path, scope: Scope) -> Result<Skill, Refusal> {
    let file = dir.join(SKILL);
    let text = std::fs::read_to_string(&file).map_err(|error| Refusal {
        at: Where::File(file.clone()),
        line: None,
        reason: format!("the file cannot be read: {error}"),
    })?;
    let read = frontmatter::parse(&text).map_err(|problem| Refusal {
        at: Where::File(file.clone()),
        line: Some(problem.line),
        reason: problem.reason,
    })?;
    Ok(Skill::new(
        read,
        scope,
        Content::Directory(dir.to_path_buf()),
    ))
}

/// A link whose target is gone, or whose own target cannot be read. It is a
/// diagnostic and not a failure: a collection shared by symlink loses a target
/// far more often than it loses its owner's interest, and the rest of the
/// catalog is still worth reading.
fn broken_link(path: &Path) -> Option<Refusal> {
    let link = std::fs::symlink_metadata(path).ok()?;
    if !link.file_type().is_symlink() {
        return None;
    }
    let target = std::fs::read_link(path);
    // The link is named together with the way it fails, so a loop and a target
    // that is simply gone are not said in the same words.
    let why = match std::fs::metadata(path) {
        Ok(_) => return None,
        Err(error) => error,
    };
    Some(Refusal {
        at: Where::File(path.to_path_buf()),
        line: None,
        reason: broken_reason(target, &why),
    })
}

/// The sentence a broken link is refused in. A link whose own target cannot be
/// read has no target to name, so it names the error instead of printing an
/// empty one: a reader of `sindri doctor` has to learn what to fix, and
/// `read_link`'s error is the evidence of why. That is also why the error the
/// follow gave up is not printed beside it — it would arrive with no target to
/// hang on, which is the sentence this function exists to stop writing.
fn broken_reason(target: std::io::Result<PathBuf>, why: &std::io::Error) -> String {
    match target {
        Ok(target) => format!("a broken link to {}: {why}", target.display()),
        Err(error) => format!("a link whose target cannot be read: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::broken_reason;
    use std::io;
    use std::path::PathBuf;

    /// `EACCES`, as a link in a directory the process may not search answers
    /// it: the only error that leaves a link with no target to print.
    fn unreadable() -> io::Error {
        io::Error::from(io::ErrorKind::PermissionDenied)
    }

    /// `ENOENT`, as a link to a target that is simply gone answers it.
    fn gone() -> io::Error {
        io::Error::from(io::ErrorKind::NotFound)
    }

    /// The sentence is pinned whole, against the error's own `Display` rather
    /// than the words this machine's `strerror` chose for it.
    #[test]
    fn a_link_whose_target_cannot_be_read_names_the_error_and_no_empty_target() {
        let reason = broken_reason(Err(unreadable()), &gone());

        assert_eq!(
            reason,
            format!("a link whose target cannot be read: {}", unreadable()),
            "no target is printed, and the link's own error says why"
        );
    }

    #[test]
    fn a_link_to_a_target_that_is_gone_is_named_with_its_target() {
        let reason = broken_reason(Ok(PathBuf::from("/shared/gone")), &gone());

        assert!(
            reason.starts_with("a broken link to /shared/gone: ")
                && reason.ends_with(&gone().to_string()),
            "a target that was read is the word the reader is looking for: {reason}"
        );
    }
}
