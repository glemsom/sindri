//! The walk is the `ignore` crate's, which reads a tree as `git` does, with two of its
//! defaults turned around: `hidden(false)`, because a tracked file may be a dotfile, and
//! `require_git(false)`, because a workspace that is no checkout still has a `.gitignore`.

use std::path::Path;

use crate::composer::Key;
use crate::ink::{Ink, Look};
use crate::kept::{CAP, Kept, Move, Narrow, leaves};
use crate::list::{self, Item, ScreenRow};

/// The one line the picker says in place of its rows when no file answers.
const NO_FILE: &str = "no file answers";

/// A row of the picker: the file's name, and the directory it sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row<'a> {
    pub name: &'a str,
    pub directory: &'a str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picker {
    files: Vec<String>,
    narrow: Narrow,
}

impl Picker {
    pub fn walk(root: &Path) -> Picker {
        let mut files: Vec<String> = Vec::new();
        let mut walk = ignore::WalkBuilder::new(root);
        walk.hidden(false).require_git(false);
        for entry in walk.build().flatten() {
            let path = entry.path();
            if entry.file_type().is_none_or(|kind| kind.is_dir()) {
                continue;
            }
            let Some(relative) = path.strip_prefix(root).ok().and_then(|rest| rest.to_str()) else {
                continue;
            };
            if relative.split('/').any(|step| step == GIT_DIRECTORY) {
                continue;
            }
            files.push(relative.to_owned());
        }
        // The walk hands siblings back in whatever order the filesystem keeps them, and the rank keeps
        // the order it is handed, so the files are sorted here and nowhere else.
        files.sort_unstable();
        Picker {
            files,
            narrow: Narrow::default(),
        }
    }

    pub fn query(&self) -> &str {
        self.narrow.query()
    }

    /// The first `CAP` matching paths, each split into the file's name and its directory.
    pub fn rows(&self) -> Vec<Row<'_>> {
        self.ranked().into_iter().take(CAP).map(split).collect()
    }

    /// Every path the query leaves, in the one order the rank gives.
    fn ranked(&self) -> Vec<&str> {
        leaves(
            self.files.iter().map(|path| (path.as_str(), path.as_str())),
            self.narrow.query(),
        )
    }

    pub fn selected(&self) -> usize {
        self.narrow.mark()
    }

    /// How many files the query leaves, whether or not the card has a row for them.
    pub fn matches(&self) -> usize {
        count(&self.files, self.narrow.query())
    }

    pub fn key(&mut self, key: Key) -> bool {
        let Picker { files, narrow } = self;
        let files = files.as_slice();
        narrow.typed(key, |query| count(files, query).min(CAP))
    }

    /// The rows the picker's card holds: the query at the head, one row per file with the row the
    /// reader is on marked, and under them the count of the matches it shows.
    pub fn body(&self) -> Vec<ScreenRow> {
        let files = self.rows();
        let items = files
            .iter()
            .map(|file| match file.directory.is_empty() {
                true => Item::new(file.name),
                false => Item::beside(file.name, file.directory),
            })
            .collect();
        let mut rows = list::List {
            query: Some(self.query()),
            said: files.is_empty().then_some(NO_FILE),
            items,
            selected: self.selected(),
            marker: list::MARK,
            chooses: true,
        }
        .rows();
        if !files.is_empty() {
            let counted = format!("1\u{2013}{} of {}", files.len(), self.matches());
            rows.push(list::whole(&counted, Look::plain(Ink::Dim)));
        }
        rows
    }

    pub fn word(&self) -> Option<String> {
        let path = *self.ranked().get(self.narrow.mark())?;
        Some(format!("{} ", shell_word(path)))
    }
}

/// How many of `files` the query leaves, whether or not the card has a row for them.
fn count(files: &[String], query: &str) -> usize {
    leaves(files.iter().map(|path| (path.as_str(), ())), query).len()
}

/// The one spelling a single-quoted word cannot swallow is `'\''`, so every quote inside it
/// is written that way. A path whose first glyph is `-` is spelled `./-v.rs`, because a word
/// a shell reads as a flag is a word that is not a file.
pub fn shell_word(path: &str) -> String {
    let path = match path.starts_with('-') {
        true => format!("./{path}"),
        false => path.to_owned(),
    };
    if path.chars().all(leaves_alone) {
        return path;
    }
    let mut word = String::from("'");
    for glyph in path.chars() {
        match glyph {
            '\'' => word.push_str("'\\''"),
            _ => word.push(glyph),
        }
    }
    word.push('\'');
    word
}

fn leaves_alone(glyph: char) -> bool {
    glyph.is_ascii_alphanumeric() || matches!(glyph, '.' | '_' | '-' | '/')
}

/// A directory in a working tree and a file in a worktree: a checkout's own bookkeeping and
/// never something a reader points a prompt at.
const GIT_DIRECTORY: &str = ".git";

/// The file's name and the directory it sits in, which spell the path back with a `/` between them.
fn split(path: &str) -> Row<'_> {
    match path.rsplit_once('/') {
        Some((directory, name)) => Row { name, directory },
        None => Row {
            name: path,
            directory: "",
        },
    }
}

impl Kept for Picker {
    fn walked(&mut self, way: Move) -> bool {
        self.narrow.walked(way, self.rows().len())
    }

    fn typed(&mut self, key: Key) -> bool {
        self.key(key)
    }

    fn pressed(&mut self, at: usize) -> bool {
        self.narrow.pressed(at, self.rows().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(files: &[&str]) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("a temp workspace");
        for file in files {
            let path = root.path().join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("the directories a file sits in");
            }
            std::fs::write(&path, "").expect("a file");
        }
        root
    }

    fn offered(picker: &Picker) -> Vec<String> {
        picker
            .rows()
            .into_iter()
            .map(|row| match row.directory {
                "" => row.name.to_owned(),
                directory => format!("{directory}/{}", row.name),
            })
            .collect()
    }

    fn narrowed(root: &Path, query: &str) -> Picker {
        let mut picker = Picker::walk(root);
        for glyph in query.chars() {
            picker.key(Key::Char(glyph));
        }
        picker
    }

    #[test]
    fn an_ignored_file_is_never_offered_and_a_tracked_one_always_can_be() {
        let root = workspace(&["tracked.rs", "notes.log", "target/artifact.rs"]);
        std::fs::write(root.path().join(".gitignore"), "*.log\ntarget/\n").expect("a gitignore");

        let rows = offered(&Picker::walk(root.path()));

        assert!(
            rows.contains(&String::from("tracked.rs")),
            "a file git tracks is a file the reader may point at, and the walk found {rows:?}"
        );
        assert!(
            !rows
                .iter()
                .any(|row| row.ends_with(".log") || row.starts_with("target/")),
            "an ignored path is a path the run cannot read, so the reader is never offered \
             it, and the walk found {rows:?}"
        );
    }

    #[test]
    fn a_file_git_tracks_is_offered_even_when_its_name_starts_with_a_dot() {
        let root = workspace(&[".github/workflows/ci.yml"]);

        let rows = offered(&Picker::walk(root.path()));

        assert_eq!(
            rows,
            vec![String::from(".github/workflows/ci.yml")],
            "hidden is git's word to say, not a dot's: a tracked file is offered wherever \
             it is"
        );
    }

    #[test]
    fn a_checkouts_own_bookkeeping_is_never_a_row() {
        let root = workspace(&[
            ".git/config",
            ".git/refs/heads/main",
            ".gitignore",
            "rows.rs",
        ]);

        assert_eq!(
            offered(&Picker::walk(root.path())),
            vec![String::from(".gitignore"), String::from("rows.rs")],
            "`.git` is the walk's own bookkeeping and never a file to point a prompt at, \
             while a hidden file a reader may well mean — a `.gitignore` is one — is \
             offered like any other"
        );
    }

    #[test]
    fn a_row_is_the_files_name_and_the_directory_it_sits_in() {
        let root = workspace(&["docs/adr/note.md", "rows.rs"]);

        assert_eq!(
            Picker::walk(root.path()).rows(),
            vec![
                Row {
                    name: "rows.rs",
                    directory: ""
                },
                Row {
                    name: "note.md",
                    directory: "docs/adr"
                },
            ],
            "a row a reader reads is the name they are looking for with the directory it sits \
             in beside it, and a file at the top of the workspace has no directory to name"
        );
    }

    #[test]
    fn a_query_narrows_the_rows_as_it_is_typed() {
        let root = workspace(&["rows.rs", "rail.rs", "view.rs"]);

        assert_eq!(
            offered(&narrowed(root.path(), "R")),
            vec![
                String::from("rail.rs"),
                String::from("rows.rs"),
                String::from("view.rs"),
            ],
            "a query is read as part of a path, in either case, and every path here ends \
             in `.rs`"
        );
        assert_eq!(
            offered(&narrowed(root.path(), "ro")),
            vec![String::from("rows.rs")],
            "and it narrows as it is typed: every file holds an `r`, and one holds `ro`"
        );
        assert_eq!(
            offered(&narrowed(root.path(), "rows.r")),
            vec![String::from("rows.rs")],
            "until one file is left"
        );
    }

    #[test]
    fn the_screen_offers_eight_rows_and_no_more() {
        let files: Vec<String> = (0..20).map(|at| format!("file{at}.rs")).collect();
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let root = workspace(&files);

        assert_eq!(
            Picker::walk(root.path()).rows().len(),
            CAP,
            "§12 caps the list at eight rows, however many files the workspace holds"
        );
    }

    #[test]
    fn the_eight_rows_are_the_eight_the_query_names() {
        let files: Vec<String> = (0..20).map(|at| format!("file{at:02}.rs")).collect();
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let root = workspace(&files);

        assert_eq!(
            offered(&narrowed(root.path(), "file17")),
            vec![String::from("file17.rs")],
            "the cap ends the list and never the workspace: a file past the eighth row is \
             reached by the query"
        );
    }

    #[test]
    fn a_query_counts_every_file_it_leaves_and_draws_only_the_first_eight() {
        let files: Vec<String> = (0..17).map(|at| format!("file{at:02}.rs")).collect();
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let root = workspace(&files);
        let picker = narrowed(root.path(), "file");

        assert_eq!(
            picker.rows().len(),
            CAP,
            "the card has room for eight rows and draws eight"
        );
        assert_eq!(
            picker.matches(),
            17,
            "and the count says what the draw cannot: seventeen files hold the query, eight \
             of them are on the screen"
        );
    }

    #[test]
    fn a_query_that_matches_nothing_draws_no_row_and_counts_none() {
        let root = workspace(&["rows.rs", "rail.rs"]);
        let picker = narrowed(root.path(), "zzz");

        assert!(
            picker.rows().is_empty(),
            "no file holds `zzz`, so no row is drawn"
        );
        assert_eq!(picker.matches(), 0, "and none is counted");
        assert_eq!(
            picker.word(),
            None,
            "nor is there a path for Enter to spell"
        );
    }

    #[test]
    fn selecting_a_row_names_the_path_as_one_shell_word() {
        let root = workspace(&["rows.rs", "notes/a file.rs"]);
        let mut picker = Picker::walk(root.path());

        assert_eq!(
            picker.word(),
            Some(String::from("rows.rs ")),
            "a path a shell reads as one word needs no quotes, and the space after it is \
             where the next word of the prompt starts"
        );
        assert!(picker.walked(Move::Next));
        assert_eq!(
            picker.word(),
            Some(String::from("'notes/a file.rs' ")),
            "a path a shell would read as two words is quoted, so what lands in the draft \
             is the one path the reader picked"
        );
    }

    #[test]
    fn a_word_a_shell_would_eat_is_quoted_whole() {
        assert_eq!(
            shell_word("crates/sindri/src/rows.rs"),
            "crates/sindri/src/rows.rs"
        );
        assert_eq!(shell_word("a file.rs"), "'a file.rs'");
        assert_eq!(shell_word("it's.rs"), r"'it'\''s.rs'");
        assert_eq!(shell_word("$HOME.rs"), "'$HOME.rs'");
        assert_eq!(shell_word("a;b.rs"), "'a;b.rs'");
        assert_eq!(
            shell_word("-v.rs"),
            "./-v.rs",
            "a word a shell reads as a flag is a word that is not a file, so the path is \
             spelled from the working directory"
        );
    }

    #[test]
    fn a_picker_with_no_file_offers_nothing_to_point_at() {
        let root = workspace(&[]);
        let mut picker = Picker::walk(root.path());

        assert!(picker.rows().is_empty());
        assert_eq!(picker.word(), None);
        assert!(!picker.walked(Move::Next));
    }

    #[test]
    fn a_workspace_walks_as_paths_relative_to_itself() {
        let root = workspace(&["crates/sindri/src/rows.rs"]);
        let picker = Picker::walk(root.path());

        assert_eq!(
            offered(&picker),
            vec![String::from("crates/sindri/src/rows.rs")],
            "the row a reader reads and the word a selection inserts are the path as the \
             run's own working directory spells it, and neither is the temp directory's \
             absolute path"
        );
    }
}
