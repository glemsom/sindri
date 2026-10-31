//! The completion offer: an `@` token ranked against a background walk of the
//! workspace (`walk`), or a `/` name ranked against the catalogue of commands. Both
//! are drawn in the one column above the composer and taken by the same keys;
//! only the path side walks, because only the filesystem is not already in
//! hand.

use crate::command::COMMANDS;
use crate::completion::walk::visible;

pub const ROWS: usize = 8;

pub(crate) mod walk;

/// What asked for the offer: an `@` path token, or a `/` command named at the
/// start of the draft. The drawn column, the keys and the accept path are
/// shared; what is ranked behind them is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    At,
    Slash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub at: usize,
    pub typed: String,
    pub trigger: Trigger,
}

#[derive(Debug, Clone)]
pub struct Completion {
    pub at: usize,
    pub typed: String,
    pub trigger: Trigger,
    pub items: Vec<String>,
    pub chosen: usize,
    pub generation: u64,
    pub capped: bool,
}

impl Completion {
    pub fn new(query: Query, generation: u64) -> Self {
        Completion {
            at: query.at,
            typed: query.typed.clone(),
            trigger: query.trigger,
            // A catalogue is in hand, so the commands are offered at once; a
            // walk has to be asked for, and its paths arrive later.
            items: match query.trigger {
                Trigger::Slash => commands(&query.typed),
                Trigger::At => Vec::new(),
            },
            chosen: 0,
            generation,
            capped: false,
        }
    }

    /// Whether this offer already answers `query`: the same token, trigged the
    /// same way, so revisiting a keystroke asks for nothing new.
    pub fn answers(&self, query: &Query) -> bool {
        self.trigger == query.trigger && self.typed == query.typed && self.at == query.at
    }

    /// The walk this offer still needs: one for an `@` path, none for a
    /// catalogue that needs no thread.
    pub fn walk(&self) -> Option<(u64, String)> {
        matches!(self.trigger, Trigger::At).then(|| (self.generation, self.typed.clone()))
    }
}

/// The offer a draft asks for: a command named at the start of a one-row
/// draft, or else the path token of the row being typed.
pub fn ask(draft: &str, row: &str) -> Option<Query> {
    command(draft).or_else(|| path(row))
}

/// The `/` a draft opens with and the name typed after it, offered only where
/// a command can be sent: the draft starts with the slash and no whitespace has
/// followed it yet. A row ends at a newline, which is whitespace, so a draft of
/// more than one row never opens the offer — nor does a name whose argument has
/// begun, nor a `/` further in.
pub fn command(draft: &str) -> Option<Query> {
    let rest = draft.strip_prefix('/')?;
    if rest.chars().any(char::is_whitespace) {
        return None;
    }
    Some(Query {
        at: 0,
        typed: rest.to_string(),
        trigger: Trigger::Slash,
    })
}

/// The commands a fragment names, nearest first: the names it starts, then
/// the ones it sits inside, then the ones it is a subsequence of — each tier in
/// the catalogue's order, because that is the order the frame says them in. The
/// catalogue is the whole of what is offered, so a seventh command is
/// completable the moment it is added to it.
pub fn commands(typed: &str) -> Vec<String> {
    let typed = typed.to_lowercase();
    let mut scored: Vec<(usize, usize, &str)> = COMMANDS
        .iter()
        .enumerate()
        .filter_map(|(index, spec)| {
            let name = spec.name.to_lowercase();
            let tier = if typed.is_empty() || name.starts_with(&typed) {
                0
            } else if name.contains(&typed) {
                1
            } else if subsequence(&name, &typed) {
                2
            } else {
                return None;
            };
            Some((tier, index, spec.name))
        })
        .collect();
    scored.sort_unstable();
    scored
        .into_iter()
        .take(ROWS)
        .map(|(_, _, name)| format!("/{name}"))
        .collect()
}

/// The last `@` on the row being typed, and what follows it. An `@` inside a
/// word is a word: completion is triggered by a token, not by a character.
pub fn path(row: &str) -> Option<Query> {
    let at = row.rfind('@')?;
    let before = row[..at].chars().next_back();
    if before.is_some_and(|ch| !ch.is_whitespace()) {
        return None;
    }
    Some(Query {
        at: row[..at].chars().count(),
        typed: row[at + 1..].to_string(),
        trigger: Trigger::At,
    })
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

#[cfg(test)]
mod tests {
    use super::walk::visible;
    use super::{ask, command, commands, offered, path};

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn the_query_is_the_token_after_the_last_at_sign() {
        assert_eq!(
            path("look at @src/ma"),
            Some(super::Query {
                at: 8,
                typed: "src/ma".to_string(),
                trigger: super::Trigger::At,
            })
        );
        assert_eq!(path("no completion here"), None);
        assert_eq!(
            path("mail me at me@example"),
            None,
            "an `@` in a word is a word"
        );
    }

    #[test]
    fn a_slash_at_the_start_of_a_draft_offers_every_command_the_catalogue_has() {
        let query = ask("/", "").expect("a slash at the start asks for the commands");
        assert_eq!(query.at, 0, "the name starts where the slash does");
        assert_eq!(query.typed, "", "nothing is typed after it yet");
        let offered = commands(&query.typed);
        let expected: Vec<String> = crate::command::COMMANDS
            .iter()
            .map(|spec| format!("/{}", spec.name))
            .collect();
        assert_eq!(
            offered, expected,
            "every command, in the order the frame says them"
        );
        for name in &offered {
            assert!(
                !matches!(
                    crate::command::parse(name),
                    Some(crate::command::Command::Unknown(_))
                ),
                "{name} is a command the frame can send"
            );
        }
    }

    #[test]
    fn a_command_is_named_only_where_one_can_be_sent() {
        assert_eq!(
            command("/").map(|query| query.typed),
            Some(String::new()),
            "a bare slash names every command"
        );
        assert_eq!(command("/set").map(|query| query.typed), Some("set".into()));
        assert!(
            command("/export notes/ga").is_none(),
            "whitespace ends the name: the rest is an argument"
        );
        assert!(command("/ex\nmore").is_none(), "a command is one row");
        assert!(command("src/ma").is_none(), "a path is not a command");
        assert!(
            command("notes/ga").is_none(),
            "a `/` further into the draft names nothing"
        );
    }

    #[test]
    fn a_fragment_narrows_the_offer_and_the_names_it_starts_come_first() {
        assert_eq!(commands("s"), paths(&["/settings"]));
        assert!(
            commands("zzzz").is_empty(),
            "a fragment that names nothing offers nothing"
        );
        let named = commands("e");
        assert_eq!(
            named.first().map(String::as_str),
            Some("/export"),
            "the name the fragment starts comes first: {named:?}"
        );
        assert!(
            !named.contains(&"/quit".to_string()),
            "a name the fragment does not reach is not offered: {named:?}"
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
}
