use sindri_core::provider::served::Listing;

use crate::composer::Key;
use crate::ink::Ink;
use crate::kept::{Kept, Move};
use crate::list::{self, Beside, ScreenRow};

pub const CAP: usize = 8;

/// The one line the screen says in place of its rows while the wire has yet to answer.
const ASKING: &str = "asking the wire what it serves";

/// The one line it says when the wire answered serving nothing.
const NO_NAME: &str = "no served name answers";

pub type Row<'a> = (&'a str, &'a str);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Models {
    listing: Option<Listing>,
    query: String,
    selected: usize,
}

impl Models {
    pub fn new() -> Models {
        Models::default()
    }

    /// The same screen with the names the wire served, which is the screen's own copy of a fact the
    /// frame's value holds. A listing that is not the one the screen holds puts the mark back on the
    /// first row; the same listing handed again leaves the reader where they were.
    pub fn served_by(self, listing: Option<Listing>) -> Models {
        let moved = self.listing != listing;
        Models {
            listing,
            selected: match moved {
                true => 0,
                false => self.selected,
            },
            ..self
        }
    }

    pub fn waiting(&self) -> bool {
        self.listing.is_none()
    }

    pub fn hint(&self) -> Option<&str> {
        self.listing.as_ref().and_then(Listing::hint)
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn rows(&self) -> Vec<Row<'_>> {
        let Some(names) = self.listing.as_ref().and_then(Listing::names) else {
            return Vec::new();
        };
        let query = self.query.to_lowercase();
        let mut ranked: Vec<(u8, Row<'_>)> = names
            .iter()
            .filter_map(|served| {
                rank(&served.id, &query)
                    .map(|rank| (rank, (served.id.as_str(), served.label.as_str())))
            })
            .collect();
        // Stable, so names that rank alike stay in the order the wire listed them in.
        ranked.sort_by_key(|(rank, _)| *rank);
        ranked.into_iter().take(CAP).map(|(_, row)| row).collect()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The rows the screen's card holds: the query at the head, one row per served name with the row
    /// the reader is on marked, or the line it says in place of them. A wire that could not answer
    /// offers its hint rather than a refusal, because a model named by hand still runs.
    pub fn body(&self) -> Vec<ScreenRow> {
        let names = self.rows();
        let said = match self.hint() {
            _ if self.waiting() => Some(ASKING),
            Some(hint) => Some(hint),
            None if names.is_empty() => Some(NO_NAME),
            None => None,
        };
        list::List {
            query: Some(self.query()),
            said,
            items: list::columned(names.into_iter(), Ink::Plain, Beside::Own(Ink::Dim)),
            selected: self.selected(),
            marker: list::MARK,
            chooses: true,
        }
        .rows()
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

    /// The row a press landed on: the selection moves to it, and a press on no row of the list moves
    /// nothing.
    pub fn select(&mut self, at: usize) -> bool {
        if at >= self.rows().len() {
            return false;
        }
        self.selected = at;
        true
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

    pub fn chosen(&self) -> Option<&str> {
        self.rows().get(self.selected).map(|(id, _)| *id)
    }
}

impl Kept for Models {
    fn walked(&mut self, way: Move) -> bool {
        match way {
            Move::Previous => self.up(),
            Move::Next => self.down(),
        }
    }

    fn typed(&mut self, key: Key) -> bool {
        self.key(key)
    }

    fn pressed(&mut self, at: usize) -> bool {
        self.select(at)
    }
}

fn rank(id: &str, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    let id = id.to_lowercase();
    if id.starts_with(query) {
        return Some(0);
    }
    id.contains(query).then_some(1)
}
