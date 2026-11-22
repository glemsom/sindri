use crate::composer::Key;

pub const CAP: usize = 8;

/// How a query sits in a spelling, strongest first: the order of the variants is the order the rows
/// come in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    NameStarts,
    NameHolds,
    PathHolds,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Previous,
    Next,
}

pub trait Kept {
    fn walked(&mut self, way: Move) -> bool;
    fn typed(&mut self, key: Key) -> bool;
    fn pressed(&mut self, at: usize) -> bool;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Narrow {
    query: String,
    mark: usize,
}

impl Narrow {
    pub fn asked(query: impl Into<String>) -> Narrow {
        Narrow {
            query: query.into(),
            mark: 0,
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn mark(&self) -> usize {
        self.mark
    }

    pub fn restart(&mut self) {
        self.mark = 0;
    }

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

    pub fn pressed(&mut self, at: usize, rows: usize) -> bool {
        if at >= rows {
            return false;
        }
        self.mark = at;
        true
    }
}
