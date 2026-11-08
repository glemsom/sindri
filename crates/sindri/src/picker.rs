//! The `@` picker: the files of the workspace, walked with `ignore` so what a run could
//! not read the reader is never offered, ranked by how near they are to the directory the
//! run stands in, narrowed by a query typed into the screen, and capped at the eight rows
//! §12 gives the list.
//!
//! The walk is the `ignore` crate's and it is `git`'s reading of the tree: a `.gitignore`,
//! a `.ignore` and a `.git/info/exclude` all apply, so a path a run cannot read is a path
//! the reader cannot point at — "a file the run can read is a file the reader may point
//! at" (§12). Two of the crate's defaults are turned around, and both for that sentence:
//! `hidden(false)`, because a tracked file may be a hidden one (`.github/workflows/ci.yml`
//! is a file a reader may well point at) and it is `.gitignore`'s business which paths are
//! offered and not a rule about a leading dot's; and `require_git(false)`, because a
//! workspace that is no checkout still has a `.gitignore` and honouring it there costs
//! nothing. The one thing `hidden(false)` lets through that must not be: `.git` itself,
//! which is skipped by name — it is the walk's own bookkeeping and not a file to point at.
//!
//! Nothing here knows what a terminal is (ADR-0005): a key or a glyph comes in, the rows
//! and the word a selection inserts come out, and the screen that draws them is `view`'s.

use std::path::Path;

use crate::composer::Key;

/// The most rows the picker offers at once: §12's cap. It is a cap rather than a scrolling
/// window because a list a reader points at is read rather than scanned, so the list has to
/// be short enough to read; the query is what reaches the row past the cap.
pub const CAP: usize = 8;

/// The files the walk found, the query narrowing them, and the row the selection is on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picker {
    /// Every file the walk found, relative to the root, nearest first: the whole written
    /// down once, because ranking nearest-first needs every candidate to know which is
    /// nearest, and the query is a filter over this list rather than a second walk.
    files: Vec<String>,
    /// The glyphs typed into the screen: the query the list is narrowed by.
    query: String,
    /// Which of the offered rows the selection is on, counted from the nearest.
    selected: usize,
}

impl Picker {
    /// Walks `root` for the files a run could read, and ranks them by how near they are to
    /// it.
    ///
    /// The whole workspace is walked once, when the screen opens, and written down: ranking
    /// nearest-first is a fact about the whole list, so a walk per keystroke would pay for
    /// the same tree again to draw a shorter list. One walk per `@` is the price of that,
    /// and §12's cap is a cap on the rows and never on the walk.
    pub fn walk(root: &Path) -> Picker {
        let mut files: Vec<String> = Vec::new();
        let mut walk = ignore::WalkBuilder::new(root);
        walk.hidden(false).require_git(false);
        for entry in walk.build().flatten() {
            let path = entry.path();
            // A directory is not a path to point at, and the root itself is no row at all.
            if entry.file_type().is_none_or(|kind| kind.is_dir()) {
                continue;
            }
            let Some(relative) = path.strip_prefix(root).ok().and_then(|rest| rest.to_str()) else {
                // A path that is not text cannot be written into a draft that is text: the
                // word a selection inserts would be a different path.
                continue;
            };
            // `.git` is the walk's own bookkeeping — a directory in a checkout and a file
            // in a worktree — and never something a reader points a prompt at.
            if relative.split('/').any(|step| step == GIT_DIRECTORY) {
                continue;
            }
            files.push(relative.to_owned());
        }
        // Nearest first, and the path's own order where two files are as near as each
        // other, so the list a reader sees is the same list every time the screen opens.
        files.sort_by(|one, other| {
            nearness(one)
                .cmp(&nearness(other))
                .then_with(|| one.cmp(other))
        });
        Picker {
            files,
            query: String::new(),
            selected: 0,
        }
    }

    /// The query as it stands.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The rows the screen paints: the files the query leaves, nearest first, at most
    /// [`CAP`] of them.
    pub fn rows(&self) -> Vec<&str> {
        let query = self.query.to_lowercase();
        self.files
            .iter()
            .filter(|file| holds(file, &query))
            .take(CAP)
            .map(String::as_str)
            .collect()
    }

    /// Which of the offered rows the selection is on.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The filter box's own map: a glyph widens the query, a backspace narrows it, and a
    /// key a filter box has no use for moves nothing. Says whether anything changed.
    ///
    /// It is a filter box and not a second draft: the query is narrowed and never composed,
    /// so it has no caret the arrows travel and no line a `Home` reaches — the arrows are
    /// the list's, which is the whole of what the screen is for.
    pub fn key(&mut self, key: Key) -> bool {
        match key {
            Key::Char(glyph) if !glyph.is_control() => {
                self.query.push(glyph);
                self.selected = 0;
                true
            }
            Key::Backspace => {
                let erased = self.query.pop().is_some();
                if erased {
                    self.selected = 0;
                }
                erased
            }
            _ => false,
        }
    }

    /// `↑`: the row above the selection, if there is one.
    pub fn up(&mut self) -> bool {
        let moved = self.selected > 0;
        self.selected = self.selected.saturating_sub(1);
        moved
    }

    /// `↓`: the row below the selection, if there is one.
    pub fn down(&mut self) -> bool {
        let moved = self.selected.saturating_add(1) < self.rows().len();
        if moved {
            self.selected += 1;
        }
        moved
    }

    /// What selecting the row under the selection puts in the draft: the path as the run's
    /// own working directory spells it, safe to hand a shell, and with the space that ends
    /// the word — the next thing typed is another word of the prompt.
    pub fn word(&self) -> Option<String> {
        let rows = self.rows();
        let path = *rows.get(self.selected)?;
        Some(format!("{} ", shell_word(path)))
    }
}

/// The word a shell reads back as `path`: bare when the path is spelled in the glyphs a
/// shell leaves alone, and single-quoted otherwise, every quote inside it spelled `'\''`,
/// which is the one spelling a single-quoted word cannot swallow.
///
/// A path whose first glyph is `-` is spelled from the working directory, `./-v.rs`, because
/// a word a shell reads as a flag is a word that is not a file.
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

/// The glyphs a shell reads as parts of one word with no quoting at all: a letter, a digit,
/// and the four marks a path is spelled with.
fn leaves_alone(glyph: char) -> bool {
    glyph.is_ascii_alphanumeric() || matches!(glyph, '.' | '_' | '-' | '/')
}

/// The name a checkout keeps its own bookkeeping under, skipped wherever it appears: a
/// directory in a working tree, a file in a worktree, and no row either way.
const GIT_DIRECTORY: &str = ".git";

/// How near a file is to the root: one for each directory between them, the file's own name
/// counting as the nearest thing there is. Two files in one directory are as near as each
/// other, and the path's own order says which comes first.
fn nearness(path: &str) -> usize {
    path.matches('/').count()
}

/// Whether `path` holds `query` as a run of glyphs, in either case: a query is read as a
/// piece of a path and never as a spelling of it, so `rows.RS` finds `rows.rs`.
fn holds(path: &str, query: &str) -> bool {
    query.is_empty() || path.to_lowercase().contains(query)
}

#[cfg(test)]
mod tests {
    //! The picker's own rules, one test each: what the walk offers, the order it offers it
    //! in, the query that narrows it, the cap that ends it, and the word a selection puts
    //! in the draft. None of them needs a terminal, and none of them needs the network: a
    //! workspace is a temp directory the test writes.

    use super::*;

    /// A workspace of the test's own holding `files`, each written with the directories it
    /// sits in, and no checkout: a `.gitignore` is honoured in a directory that is no
    /// repository, which is what a workspace handed to a run often is.
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

    /// The rows a picker over `root` offers, as the screen spells them.
    fn offered(picker: &Picker) -> Vec<String> {
        picker.rows().into_iter().map(str::to_owned).collect()
    }

    /// A picker over `root` with `query` typed into it, a glyph at a time: the list a
    /// reader sees after typing.
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
    fn a_narrower_query_puts_the_selection_back_on_the_nearest_row() {
        let root = workspace(&["rows.rs", "rail.rs"]);
        let mut picker = Picker::walk(root.path());
        picker.down();
        assert_eq!(picker.selected(), 1, "the second row is selected");

        picker.key(Key::Char('r'));

        assert_eq!(
            picker.selected(),
            0,
            "a query that leaves a shorter list cannot keep a row that may be gone, and the \
             nearest file the query leaves is the one a reader means"
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
