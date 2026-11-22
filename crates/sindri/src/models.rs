use sindri_core::provider::served::Listing;

use crate::composer::Key;
use crate::ink::Ink;
use crate::kept::{CAP, Kept, Move, Narrow, leaves};
use crate::list::{self, Beside, ScreenRow};

const ASKING: &str = "asking the wire what it serves";

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
        leaves(self.names(), self.narrow.query())
            .into_iter()
            .take(CAP)
            .collect()
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
        self.narrow.mark()
    }

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
        let names = listing
            .as_ref()
            .and_then(Listing::names)
            .unwrap_or_default();
        narrow.typed(key, |query| {
            leaves(names.iter().map(|served| (served.id.as_str(), ())), query)
                .len()
                .min(CAP)
        })
    }

    pub fn chosen(&self) -> Option<&str> {
        self.rows().get(self.narrow.mark()).map(|(id, _)| *id)
    }
}

impl Kept for Models {
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
