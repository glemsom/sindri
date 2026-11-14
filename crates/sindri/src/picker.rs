//! The walk is the `ignore` crate's, which reads a tree as `git` does, with two of its
//! defaults turned around: `hidden(false)`, because a tracked file may be a dotfile, and
//! `require_git(false)`, because a workspace that is no checkout still has a `.gitignore`.

use std::path::Path;

use crate::composer::Key;

pub const CAP: usize = 8;

/// A row of the picker: the file's name, and the directory it sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row<'a> {
    pub name: &'a str,
    pub directory: &'a str,
}

/// How a query sits in a path, strongest first: the order of the variants is the order the rows
/// come in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    NameStarts,
    NameHolds,
    PathHolds,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picker {
    files: Vec<String>,
    query: String,
    selected: usize,
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
        Picker {
            files,
            query: String::new(),
            selected: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// The first `CAP` matching paths, each split into the file's name and its directory.
    pub fn rows(&self) -> Vec<Row<'_>> {
        self.ranked().into_iter().take(CAP).map(split).collect()
    }

    /// Every path the query leaves, in one total order: rank, then nearness, then the path itself.
    fn ranked(&self) -> Vec<&str> {
        let query = self.query.to_lowercase();
        let mut ranked: Vec<(Rank, usize, &str)> = self
            .files
            .iter()
            .filter_map(|path| Some((rank(path, &query)?, nearness(path), path.as_str())))
            .collect();
        ranked.sort_unstable();
        ranked.into_iter().map(|(_, _, path)| path).collect()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// How many files the query leaves, whether or not the card has a row for them.
    pub fn matches(&self) -> usize {
        let query = self.query.to_lowercase();
        self.files
            .iter()
            .filter(|path| rank(path, &query).is_some())
            .count()
    }

    pub fn key(&mut self, key: Key) -> bool {
        match key {
            Key::Char(glyph) if !glyph.is_control() => {
                self.query.push(glyph);
                self.reselect();
                true
            }
            Key::Backspace => {
                let erased = self.query.pop().is_some();
                if erased {
                    self.reselect();
                }
                erased
            }
            _ => false,
        }
    }

    /// An edit re-ranks the rows, so one the query took away sends the selection back to the
    /// nearest row; a row the query still leaves keeps it where the reader put it.
    fn reselect(&mut self) {
        if self.selected >= self.rows().len() {
            self.selected = 0;
        }
    }

    pub fn up(&mut self) -> bool {
        let moved = self.selected > 0;
        self.selected = self.selected.saturating_sub(1);
        moved
    }

    pub fn down(&mut self) -> bool {
        let moved = self.selected.saturating_add(1) < self.rows().len();
        if moved {
            self.selected += 1;
        }
        moved
    }

    pub fn word(&self) -> Option<String> {
        let path = *self.ranked().get(self.selected)?;
        Some(format!("{} ", shell_word(path)))
    }
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

fn nearness(path: &str) -> usize {
    path.matches('/').count()
}

/// How `query`, read as lowercase, sits in `path`, or none when it does not sit in it at all: an
/// empty query is a query every name starts.
fn rank(path: &str, query: &str) -> Option<Rank> {
    if query.is_empty() {
        return Some(Rank::NameStarts);
    }
    let name = split(path).name.to_lowercase();
    if name.starts_with(query) {
        return Some(Rank::NameStarts);
    }
    if name.contains(query) {
        return Some(Rank::NameHolds);
    }
    path.to_lowercase()
        .contains(query)
        .then_some(Rank::PathHolds)
}

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
    fn the_files_nearest_the_working_directory_come_first() {
        let root = workspace(&["deep/middle/near.rs", "near.rs"]);
        let picker = Picker::walk(root.path());

        assert_eq!(
            offered(&picker),
            vec![String::from("near.rs"), String::from("deep/middle/near.rs")],
            "proximity is the ranking: a file in the directory the run stands in is nearer \
             than one three directories down, and it comes first"
        );
    }

    #[test]
    fn a_name_the_query_starts_outranks_a_name_it_holds_and_a_path_it_holds_comes_last() {
        let root = workspace(&["x/y/z/late.rs", "src/a/deep.rs", "app.rs"]);

        assert_eq!(
            offered(&narrowed(root.path(), "a")),
            vec![
                String::from("app.rs"),
                String::from("x/y/z/late.rs"),
                String::from("src/a/deep.rs"),
            ],
            "a name the query starts is a name a reader is typing, so it comes first; a name \
             that holds the query comes next; and a path that holds it only in its \
             directories comes last, however near the file sits"
        );
    }

    #[test]
    fn two_matches_of_one_rank_come_in_one_order_every_time() {
        let root = workspace(&["zebra.rs", "alpha.rs"]);
        let once = offered(&Picker::walk(root.path()));
        let twice = offered(&Picker::walk(root.path()));

        assert_eq!(
            once,
            vec![String::from("alpha.rs"), String::from("zebra.rs")],
            "the walk hands back siblings in whatever order the filesystem keeps them, so two \
             names of one rank are ordered here: the path is the last word on it, and \
             `alpha.rs` sorts before `zebra.rs`"
        );
        assert_eq!(
            twice, once,
            "and a second walk of the same workspace offers the same list in the same order, \
             because nothing about the order is left to the filesystem"
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
    fn the_selection_stays_on_a_drawn_row_as_the_query_narrows_widens_and_empties() {
        let root = workspace(&["rock.rs", "rover.rs", "rows.rs", "view.rs"]);
        let mut picker = Picker::walk(root.path());
        picker.down();
        picker.down();
        assert_eq!(picker.selected(), 2, "the third of four rows is selected");

        for (query, drawn) in [("o", 3), ("ro", 3), ("row", 1), ("", 4)] {
            while !picker.query().is_empty() {
                picker.key(Key::Backspace);
            }
            for glyph in query.chars() {
                picker.key(Key::Char(glyph));
            }
            assert_eq!(picker.rows().len(), drawn, "`{query}` leaves {drawn} rows");
            assert!(
                picker.selected() < picker.rows().len(),
                "the query `{query}` leaves {drawn} rows, and the selection stands on one of \
                 them rather than on a row the query took away"
            );
        }
    }

    #[test]
    fn the_arrows_walk_the_rows_and_stop_at_the_ends() {
        let root = workspace(&["one.rs", "two.rs"]);
        let mut picker = Picker::walk(root.path());

        assert_eq!(picker.selected(), 0, "a picker opens on the nearest file");
        assert!(
            !picker.up(),
            "there is no row above the first, so `↑` moves nothing and nothing is painted"
        );
        assert!(picker.down());
        assert_eq!(picker.selected(), 1);
        assert!(!picker.down(), "nor below the last");
        assert!(picker.up());
        assert_eq!(picker.selected(), 0);
    }

    #[test]
    fn a_query_that_still_leaves_the_selected_row_keeps_the_selection_on_it() {
        let root = workspace(&["rows.rs", "rail.rs"]);
        let mut picker = Picker::walk(root.path());
        picker.down();
        assert_eq!(
            picker.rows()[1].name,
            "rows.rs",
            "the selection stands on the second row, which is `rows.rs`"
        );

        picker.key(Key::Char('r'));

        assert_eq!(
            picker.selected(),
            1,
            "`r` leaves both files in the order they stood, so the row the reader pointed at \
             is still drawn and the selection stays on it"
        );
        assert_eq!(
            picker.rows()[1].name,
            "rows.rs",
            "which is still the file the reader pointed at"
        );
    }

    #[test]
    fn a_query_that_takes_the_selected_row_away_puts_the_selection_back_on_the_nearest_row() {
        let root = workspace(&["rows.rs", "rail.rs"]);
        let mut picker = Picker::walk(root.path());
        picker.down();

        picker.key(Key::Char('o'));

        assert_eq!(
            picker.rows().len(),
            1,
            "`o` leaves `rows.rs` and nothing else, so the row the selection stood on is gone"
        );
        assert_eq!(
            picker.selected(),
            0,
            "and a row the query took away cannot hold the selection: the nearest row the \
             query leaves is the one a reader means"
        );
    }

    #[test]
    fn a_backspace_opens_the_list_back_up() {
        let root = workspace(&["rows.rs", "rail.rs", "view.rs"]);

        let mut picker = narrowed(root.path(), "ro");
        assert_eq!(offered(&picker), vec![String::from("rows.rs")]);

        assert!(picker.key(Key::Backspace));
        assert_eq!(
            offered(&picker),
            vec![
                String::from("rail.rs"),
                String::from("rows.rs"),
                String::from("view.rs"),
            ],
            "erasing a glyph of the query widens the list again"
        );
        assert_eq!(picker.query(), "r");
    }

    #[test]
    fn a_key_a_filter_box_has_no_use_for_moves_nothing() {
        let root = workspace(&["rows.rs"]);
        let mut picker = Picker::walk(root.path());

        for key in [Key::Left, Key::Home, Key::Enter, Key::Quit, Key::Cancel] {
            assert!(
                !picker.key(key),
                "{key:?} is not a glyph and not an erasure, so the filter box is not the \
                 key's to answer"
            );
        }
        assert_eq!(picker.query(), "");
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
        picker.down();
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
        assert!(!picker.down());
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
