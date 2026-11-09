//! The walk is the `ignore` crate's, which reads a tree as `git` does, with two of its
//! defaults turned around: `hidden(false)`, because a tracked file may be a dotfile, and
//! `require_git(false)`, because a workspace that is no checkout still has a `.gitignore`.

use std::path::Path;

use crate::composer::Key;

pub const CAP: usize = 8;

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

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn rows(&self) -> Vec<&str> {
        let query = self.query.to_lowercase();
        self.files
            .iter()
            .filter(|file| holds(file, &query))
            .take(CAP)
            .map(String::as_str)
            .collect()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

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
        let rows = self.rows();
        let path = *rows.get(self.selected)?;
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

fn holds(path: &str, query: &str) -> bool {
    query.is_empty() || path.to_lowercase().contains(query)
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
        picker.rows().into_iter().map(str::to_owned).collect()
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
