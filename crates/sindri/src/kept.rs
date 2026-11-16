//! The list a screen keeps: what a cursor movement walks, what a typed glyph reaches and which row a
//! press chooses — the picker's files, the wire's served names, the config's keys and the commands
//! card a draft opens answering alike.

use crate::composer::Key;

/// The way a list's mark moves, which `↑` and `↓` are on every screen that keeps one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Previous,
    Next,
}

/// The rows a screen keeps, as the frame asks them: the picker's files, the wire's served names, the
/// config's keys and the card a draft opens. One answer each, so a screen that gains an action is
/// found in its description and nowhere else.
pub trait Kept {
    /// The mark a way along, and whether it moved.
    fn walked(&mut self, way: Move) -> bool;
    /// A typed glyph or an erasure, and whether the rows changed.
    fn typed(&mut self, key: Key) -> bool;
    /// The row a press landed on, marked, and whether the list still holds it.
    fn pressed(&mut self, at: usize) -> bool;
}

/// The query a reader types at a list and the mark they walk: held once, so a glyph typed, a glyph
/// erased and a movement are one act on every screen, and a mark an edit takes its row from goes to
/// the nearest row the query leaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Narrow {
    query: String,
    mark: usize,
}

impl Narrow {
    /// The list asked with `query`, the mark on the first row it leaves.
    pub fn asked(query: impl Into<String>) -> Narrow {
        Narrow {
            query: query.into(),
            mark: 0,
        }
    }

    /// The query, as the reader typed it.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The row the mark stands on.
    pub fn mark(&self) -> usize {
        self.mark
    }

    /// The mark back on the first row, for a screen handed a list that is not the one it held.
    pub fn restart(&mut self) {
        self.mark = 0;
    }

    /// A typed glyph or an erasure, and whether the query changed. `rows` answers the rows a query
    /// leaves as the screen draws them, each as the word it spells; the mark stands on the row it
    /// stood on where they still hold it, and on the nearest of them where they do not.
    pub fn typed<'a>(&mut self, key: Key, rows: impl Fn(&str) -> Vec<&'a str>) -> bool {
        let stood = rows(&self.query)
            .get(self.mark)
            .map(|spelt| spelt.to_string());
        let changed = match key {
            Key::Char(glyph) if !glyph.is_control() => {
                self.query.push(glyph);
                true
            }
            Key::Backspace => self.query.pop().is_some(),
            _ => false,
        };
        if !changed {
            return false;
        }
        let rows = rows(&self.query);
        self.mark = match stood.and_then(|spelt| rows.iter().position(|row| *row == spelt)) {
            Some(at) => at,
            None => self.mark.min(rows.len().saturating_sub(1)),
        };
        true
    }

    /// The mark a way along the `rows` a screen holds, and whether it moved: no row above the first
    /// and none below the last, so the ends are where a list stops.
    pub fn walked(&mut self, way: Move, rows: usize) -> bool {
        let at = match way {
            Move::Previous => self.mark.checked_sub(1),
            Move::Next => (self.mark + 1 < rows).then(|| self.mark + 1),
        };
        match at {
            Some(at) => {
                self.mark = at;
                true
            }
            None => false,
        }
    }

    /// The mark on the row a press landed on, where the `rows` a screen holds still hold it.
    pub fn pressed(&mut self, at: usize, rows: usize) -> bool {
        if at >= rows {
            return false;
        }
        self.mark = at;
        true
    }
}
