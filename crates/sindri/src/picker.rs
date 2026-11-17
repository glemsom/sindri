//! The `@` screen: the workspace's files narrowed by the query typed at it, the rows a card shows
//! of what the query leaves, and the word a selection puts in the draft. The files themselves are
//! the workspace's own walk, held rather than repeated.

use std::path::Path;

use crate::composer::Key;
use crate::files::Files;
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

/// The `@` screen: the files of the workspace it was opened on, and the query typed at them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picker {
    files: Files,
    narrow: Narrow,
}

impl Picker {
    pub fn walk(root: &Path) -> Picker {
        Picker::over(Files::walk(root))
    }

    /// The same screen over a walk already made: the frame's own, so the rows a query leaves and the
    /// count the band's edge hangs are never of two workspaces.
    pub fn over(files: Files) -> Picker {
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
            self.files
                .paths()
                .iter()
                .map(|path| (path.as_str(), path.as_str())),
            self.narrow.query(),
        )
    }

    pub fn selected(&self) -> usize {
        self.narrow.mark()
    }

    /// How many files the query leaves, whether or not the card has a row for them.
    pub fn matches(&self) -> usize {
        count(self.files.paths(), self.narrow.query())
    }

    pub fn key(&mut self, key: Key) -> bool {
        let Picker { files, narrow } = self;
        let files = files.paths();
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
mod tests;
