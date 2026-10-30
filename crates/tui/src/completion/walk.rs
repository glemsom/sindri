//! The `@` side's own machinery: a capped walk of the workspace, and the rule
//! for which of the paths it found may be offered. A catalogue needs none of
//! it, because the commands are in hand.

use std::path::Path;

pub const CAP: usize = 20_000;

pub fn visible(path: &str, typed: &str) -> bool {
    let typed = typed.to_lowercase();
    let dots_welcome = typed.starts_with('.') || typed.split('/').any(|part| part.starts_with('.'));
    path.split('/')
        .all(|part| dots_welcome || !part.starts_with('.'))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    pub paths: Vec<String>,
    pub capped: bool,
}

/// Walks `root` for the paths `@` can offer. `.git` is skipped; `.gitignore`
/// is deliberately not honoured, because a file the model can read is a file
/// the user can point at.
pub fn scan(root: &Path, cap: usize) -> Scan {
    let mut found = Scan::default();
    let mut pending = vec![String::new()];
    while let Some(directory) = pending.pop() {
        let here = if directory.is_empty() {
            root.to_path_buf()
        } else {
            root.join(&directory)
        };
        let Ok(entries) = std::fs::read_dir(&here) else {
            continue;
        };
        let mut names: Vec<(String, bool)> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().ok()?.is_dir();
                Some((name, is_dir))
            })
            .filter(|(name, _)| name != ".git")
            .collect();
        names.sort();
        for (name, is_dir) in names {
            let path = if directory.is_empty() {
                name.clone()
            } else {
                format!("{directory}/{name}")
            };
            if found.paths.len() >= cap {
                found.capped = true;
                return found;
            }
            if is_dir {
                found.paths.push(format!("{path}/"));
                pending.push(path);
            } else {
                found.paths.push(path);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::scan;

    #[test]
    fn the_walk_skips_git_honours_nothing_else_and_stops_at_the_cap() {
        let root = tempfile::tempdir().expect("a workspace");
        std::fs::write(root.path().join(".gitignore"), "build/\n").expect("a gitignore");
        std::fs::create_dir_all(root.path().join(".git/objects")).expect("a git directory");
        std::fs::create_dir_all(root.path().join("build")).expect("a build directory");
        std::fs::write(root.path().join("build/out.o"), "").expect("an ignored file");
        std::fs::write(root.path().join("main.rs"), "").expect("a source file");
        std::fs::write(root.path().join("src.rs"), "").expect("a source file");

        let found = scan(root.path(), super::CAP);
        assert!(found.paths.contains(&"main.rs".to_string()));
        assert!(
            !found.capped,
            "a small tree is walked whole: {:?}",
            found.paths
        );
        assert!(
            found
                .paths
                .iter()
                .all(|path| !path.starts_with(".git/") && path != ".git/"),
            "`.git` is skipped: {:?}",
            found.paths
        );
        assert!(
            found.paths.contains(&"build/out.o".to_string()),
            "`.gitignore` is deliberately not honoured: {:?}",
            found.paths
        );

        let capped = scan(root.path(), 2);
        assert_eq!(capped.paths.len(), 2);
        assert!(capped.capped, "the cap says when it stopped the walk");
    }
}
