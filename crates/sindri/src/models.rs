use sindri_core::provider::served::Listing;

use crate::composer::Key;

pub const CAP: usize = 8;

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

    pub fn served(&mut self, listing: Listing) {
        self.listing = Some(listing);
        self.selected = 0;
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

    pub fn chosen(&self) -> Option<&str> {
        self.rows().get(self.selected).map(|(id, _)| *id)
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
