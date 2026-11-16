use sindri_core::provider::served::Listing;

use crate::composer::Key;
use crate::ink::Ink;
use crate::kept::{Kept, Move, Narrow};
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
    narrow: Narrow,
}

impl Models {
    pub fn new() -> Models {
        Models::default()
    }

    /// The same screen with the names the wire served, which is the screen's own copy of a fact the
    /// frame's value holds. A listing that is not the one the screen holds puts the mark back on the
    /// first row; the same listing handed again leaves the reader where they were.
    pub fn served_by(mut self, listing: Option<Listing>) -> Models {
        if self.listing != listing {
            self.listing = listing;
            self.narrow.restart();
        }
        self
    }

    pub fn waiting(&self) -> bool {
        self.listing.is_none()
    }

    pub fn hint(&self) -> Option<&str> {
        self.listing.as_ref().and_then(Listing::hint)
    }

    pub fn query(&self) -> &str {
        self.narrow.query()
    }

    pub fn rows(&self) -> Vec<Row<'_>> {
        leaves(self.listing.as_ref(), self.narrow.query())
    }

    pub fn selected(&self) -> usize {
        self.narrow.mark()
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
        let Models { listing, narrow } = self;
        let listing = listing.as_ref();
        narrow.typed(key, |query| leaves(listing, query).len())
    }

    pub fn chosen(&self) -> Option<&str> {
        self.rows().get(self.narrow.mark()).map(|(id, _)| *id)
    }
}

/// The names `query` leaves, ranked and in the wire's own order for ties, the first `CAP` of them.
fn leaves<'a>(listing: Option<&'a Listing>, query: &str) -> Vec<Row<'a>> {
    let Some(names) = listing.and_then(Listing::names) else {
        return Vec::new();
    };
    let query = query.to_lowercase();
    let mut ranked: Vec<(u8, Row<'a>)> = names
        .iter()
        .filter_map(|served| {
            rank(&served.id, &query).map(|rank| (rank, (served.id.as_str(), served.label.as_str())))
        })
        .collect();
    // Stable, so names that rank alike stay in the order the wire listed them in.
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().take(CAP).map(|(_, row)| row).collect()
}

impl Kept for Models {
    fn walked(&mut self, way: Move) -> bool {
        let rows = self.rows().len();
        self.narrow.walked(way, rows)
    }

    fn typed(&mut self, key: Key) -> bool {
        self.key(key)
    }

    fn pressed(&mut self, at: usize) -> bool {
        let rows = self.rows().len();
        self.narrow.pressed(at, rows)
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
