use sindri_core::provider::served::Listing;

use crate::composer::Key;
use crate::ink::{Ink, Look};
use crate::kept::{CAP, Kept, Move, Narrow, leaves};
use crate::list::{self, Beside, ScreenRow};

const ASKING: &str = "asking the wire what it serves";

const NO_NAME: &str = "no served name answers";

pub type Row<'a> = (&'a str, &'a str);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Models {
    listing: Option<Listing>,
    narrow: Narrow,
    top: usize,
}

impl Models {
    pub fn new() -> Models {
        Models::default()
    }

    pub fn served_by(mut self, listing: Option<Listing>) -> Models {
        if self.listing != listing {
            self.listing = listing;
            self.narrow.restart();
            self.top = 0;
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
        self.ranked().into_iter().skip(self.top).take(CAP).collect()
    }

    fn ranked(&self) -> Vec<Row<'_>> {
        leaves(self.names(), self.narrow.query())
    }

    fn reveal(&mut self) {
        self.top = self.top.min(self.narrow.mark());
        self.top = self.top.max((self.narrow.mark() + 1).saturating_sub(CAP));
        self.top = self.top.min(self.ranked().len().saturating_sub(CAP));
    }

    fn names(&self) -> impl Iterator<Item = (&str, Row<'_>)> + '_ {
        self.listing
            .as_ref()
            .and_then(Listing::names)
            .unwrap_or_default()
            .iter()
            .map(|served| {
                (
                    served.id.as_str(),
                    (served.id.as_str(), served.label.as_str()),
                )
            })
    }

    pub fn selected(&self) -> usize {
        self.narrow.mark().saturating_sub(self.top)
    }

    pub fn body(&self) -> Vec<ScreenRow> {
        let held = self.rows();
        let said = match self.hint() {
            _ if self.waiting() => Some(ASKING),
            Some(hint) => Some(hint),
            None if held.is_empty() => Some(NO_NAME),
            None => None,
        };
        let shown = held.len();
        let mut rows = list::List {
            query: Some(self.query()),
            said,
            items: list::columned(held.into_iter(), Ink::Plain, Beside::Own(Ink::Dim)),
            selected: self.selected(),
            marker: list::MARK,
            chooses: true,
        }
        .rows();
        let matches = self.ranked().len();
        if matches > CAP {
            rows.push(list::whole(
                &format!("{}–{} of {matches}", self.top + 1, self.top + shown),
                Look::plain(Ink::Dim),
            ));
        }
        rows
    }

    pub fn key(&mut self, key: Key) -> bool {
        let Models {
            listing, narrow, ..
        } = self;
        let names = listing
            .as_ref()
            .and_then(Listing::names)
            .unwrap_or_default();
        let changed = narrow.typed(key, |query| {
            leaves(names.iter().map(|served| (served.id.as_str(), ())), query).len()
        });
        if changed {
            self.top = 0;
            self.reveal();
        }
        changed
    }

    pub fn chosen(&self) -> Option<&str> {
        self.ranked().get(self.narrow.mark()).map(|(id, _)| *id)
    }
}

impl Kept for Models {
    fn walked(&mut self, way: Move) -> bool {
        let changed = self.narrow.walked(way, self.ranked().len());
        self.reveal();
        changed
    }

    fn typed(&mut self, key: Key) -> bool {
        self.key(key)
    }

    fn pressed(&mut self, at: usize) -> bool {
        let changed = self.narrow.pressed(self.top + at, self.ranked().len());
        self.reveal();
        changed
    }
}
