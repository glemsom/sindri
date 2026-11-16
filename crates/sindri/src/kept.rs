//! The list a screen keeps: the query a reader types at it, the mark they walk, the rule that puts the
//! mark back when an edit takes the row it stood on away, the rows a query leaves and the cap a card
//! shows — the picker's files, the wire's served names, the config's keys and the commands alike.

use crate::composer::Key;

/// How many rows a card shows of what its query leaves, where a screen holds more than one.
pub const CAP: usize = 8;

/// How a query sits in a spelling, strongest first: the order of the variants is the order the rows
/// come in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    NameStarts,
    NameHolds,
    PathHolds,
}

/// The rows `items` hands the rank that `query` leaves, nearest first within one rank: each item is
/// the spelling a query is read against and the row handed back, a name is that spelling after its
/// last `/`, and the rows of one rank keep the order they were handed in.
pub fn leaves<'a, T>(items: impl IntoIterator<Item = (&'a str, T)>, query: &str) -> Vec<T> {
    let query = query.to_lowercase();
    let mut ranked: Vec<(Rank, usize, T)> = items
        .into_iter()
        .filter_map(|(spelling, row)| Some((rank(spelling, &query)?, nearness(spelling), row)))
        .collect();
    ranked.sort_by_key(|(rank, nearness, _)| (*rank, *nearness));
    ranked.into_iter().map(|(_, _, row)| row).collect()
}

fn name(spelling: &str) -> &str {
    match spelling.rsplit_once('/') {
        Some((_, name)) => name,
        None => spelling,
    }
}

fn nearness(spelling: &str) -> usize {
    spelling.matches('/').count()
}

/// How `query`, read as lowercase, sits in `spelling`, or none when it does not sit in it at all: an
/// empty query is a query every name starts.
fn rank(spelling: &str, query: &str) -> Option<Rank> {
    if query.is_empty() {
        return Some(Rank::NameStarts);
    }
    let name = name(spelling).to_lowercase();
    if name.starts_with(query) {
        return Some(Rank::NameStarts);
    }
    if name.contains(query) {
        return Some(Rank::NameHolds);
    }
    spelling
        .to_lowercase()
        .contains(query)
        .then_some(Rank::PathHolds)
}

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
    /// A typed glyph or an erasure, and whether the list changed.
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

    /// A typed glyph or an erasure, and whether the query changed. `rows` answers how many rows a
    /// query leaves, and the mark keeps the place it stood in: a row an edit took away sends the
    /// mark to the nearest one left.
    pub fn typed(&mut self, key: Key, rows: impl Fn(&str) -> usize) -> bool {
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
        self.mark = self.mark.min(rows(&self.query).saturating_sub(1));
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
