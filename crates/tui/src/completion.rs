//! `@` completion: a background walk of the workspace, ranked by how close
//! each path is to what was typed, capped so a build directory cannot make
//! the walk the frame's problem (ADR-0011).

use std::path::Path;

pub const ROWS: usize = 8;
pub const CAP: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub at: usize,
    pub typed: String,
}

#[derive(Debug, Clone)]
pub struct Completion {
    pub at: usize,
    pub typed: String,
    pub items: Vec<String>,
    pub chosen: usize,
    pub generation: u64,
    pub capped: bool,
}

impl Completion {
    pub fn new(query: Query, generation: u64) -> Self {
        Completion {
            at: query.at,
            typed: query.typed,
            items: Vec::new(),
            chosen: 0,
            generation,
            capped: false,
        }
    }
}

/// The last `@` on the row being typed, and what follows it. An `@` inside a
/// word is a word: completion is triggered by a token, not by a character.
pub fn query(row: &str) -> Option<Query> {
    let at = row.rfind('@')?;
    let before = row[..at].chars().next_back();
    if before.is_some_and(|ch| !ch.is_whitespace()) {
        return None;
    }
    Some(Query {
        at: row[..at].chars().count(),
        typed: row[at + 1..].to_string(),
    })
}

pub fn visible(path: &str, typed: &str) -> bool {
    let typed = typed.to_lowercase();
    let dots_welcome = typed.starts_with('.') || typed.split('/').any(|part| part.starts_with('.'));
    path.split('/')
        .all(|part| dots_welcome || !part.starts_with('.'))
}

pub fn offered(paths: &[String], typed: &str) -> Vec<String> {
    let mut scored: Vec<(usize, usize, usize, &String)> = paths
        .iter()
        .filter(|path| visible(path, typed))
        .filter_map(|path| proximity(path, typed).map(|tier| (tier, path.len(), depth(path), path)))
        .collect();
    scored.sort();
    scored
        .into_iter()
        .take(ROWS)
        .map(|(_, _, _, path)| path.clone())
        .collect()
}

fn proximity(path: &str, typed: &str) -> Option<usize> {
    let typed = typed.strip_prefix("./").unwrap_or(typed).to_lowercase();
    let path_lower = path.to_lowercase();
    if typed.is_empty() {
        return Some(3);
    }
    let name = path_lower.rsplit('/').next().unwrap_or(&path_lower);
    let last_typed = typed.rsplit('/').next().unwrap_or(&typed);
    if path_lower.starts_with(&typed) {
        return Some(0);
    }
    if name.starts_with(last_typed) {
        return Some(1);
    }
    if path_lower.contains(&typed) {
        return Some(2);
    }
    if subsequence(name, last_typed) {
        return Some(3);
    }
    subsequence(&path_lower, &typed).then_some(4)
}

fn subsequence(hay: &str, needle: &str) -> bool {
    let mut wanted = needle.chars();
    let mut next = wanted.next();
    for ch in hay.chars() {
        match next {
            None => return true,
            Some(want) if want == ch => next = wanted.next(),
            Some(_) => {}
        }
    }
    next.is_none()
}

fn depth(path: &str) -> usize {
    path.matches('/').count()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    pub paths: Vec<String>,
    pub capped: bool,
}

/// Walks `root` for the paths `@` can offer. `.git` is skipped; `.gitignore`
/// is deliberately not honoured, because a file the model can read is a file
/// the user can point at (ADR-0011).
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
    use super::{offered, query, scan, visible};

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn the_query_is_the_token_after_the_last_at_sign() {
        assert_eq!(
            query("look at @src/ma"),
            Some(super::Query {
                at: 8,
                typed: "src/ma".to_string()
            })
        );
        assert_eq!(query("no completion here"), None);
        assert_eq!(
            query("mail me at me@example"),
            None,
            "an `@` in a word is a word"
        );
    }

    #[test]
    fn a_path_that_starts_with_what_was_typed_comes_first() {
        let offered = offered(
            &paths(&["docs/main.md", "src/main.rs", "src/markdown/mod.rs"]),
            "src/ma",
        );
        assert_eq!(offered.first().map(String::as_str), Some("src/main.rs"));
        assert!(offered.contains(&"src/markdown/mod.rs".to_string()));
    }

    #[test]
    fn a_file_name_that_starts_with_the_fragment_is_nearer_than_one_that_merely_contains_it() {
        let offered = offered(&paths(&["src/xmain.rs", "src/main.rs"]), "main");
        assert_eq!(offered, paths(&["src/main.rs", "src/xmain.rs"]));
    }

    #[test]
    fn a_fragment_that_matches_nothing_is_offered_nothing() {
        assert!(offered(&paths(&["src/main.rs"]), "zzzz").is_empty());
    }

    #[test]
    fn dotfiles_wait_for_a_dot_to_be_typed() {
        assert!(!visible(".gitignore", "gi"));
        assert!(visible(".gitignore", ".gi"));
        assert!(visible("src/main.rs", "ma"));
        assert!(!visible("src/.hidden/file", "src/hi"));
        assert!(
            !visible("src/日本語/.hidden", "src/日本"),
            "a dotfile waits for a dot whatever the alphabet"
        );
        let offered = offered(&paths(&[".github/workflows/ci.yml", "gist.md"]), "gi");
        assert_eq!(offered, paths(&["gist.md"]), "no dotfile without a dot");
    }

    #[test]
    fn at_most_eight_rows_are_offered() {
        let many: Vec<String> = (0..50).map(|index| format!("src/file{index}.rs")).collect();
        assert_eq!(offered(&many, "src/f").len(), super::ROWS);
    }

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
