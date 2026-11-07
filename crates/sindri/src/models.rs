//! The `/model` screen: the names the wire serves, ranked as a query is typed into the
//! screen, and the one name `Enter` chooses (§10, §12).
//!
//! Nothing here asks a wire. A listing is the core's — read off the wire and kept beside
//! the `(wire, base_url)` pair that produced it — and this is what a frame makes of one: a
//! screen the reader narrows. The ask belongs to whoever owns the socket, and a list of
//! names written down here would be the one thing §10 forbids.
//!
//! A wire that could not answer has no names and one line saying why, which is §10's hint
//! and never a refusal: the screen paints the line where the names would go, and
//! `/model <name>` still names a model by hand.
//!
//! Nothing here knows what a terminal is (ADR-0005): a key or a glyph comes in, the rows
//! the screen offers and the name a selection takes come out.

use sindri_core::provider::served::Listing;

use crate::composer::Key;

/// The most rows the screen offers at once: the cap §12 gives the `@` picker, spent here
/// for the same reason — a list a reader points at is read rather than scanned, so what
/// reaches the ninth row is the query and never a scroll.
pub const CAP: usize = 8;

/// One row of the screen: the identifier `Enter` writes, and the wire's own prose beside
/// it, so a reader chooses between names they can read.
pub type Row<'a> = (&'a str, &'a str);

/// What the wire answered, the query narrowing it, and the row the selection is on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Models {
    /// The listing when it has landed, and `None` while the wire is being asked: a screen
    /// that is up before its names are is a screen that says it is asking.
    listing: Option<Listing>,
    /// The glyphs typed into the screen: the query the rows are ranked by.
    query: String,
    /// Which of the offered rows the selection is on, counted from the best match.
    selected: usize,
}

impl Models {
    /// A screen opened and asking: no names yet, no query, the selection on the row a
    /// ranking puts first.
    pub fn new() -> Models {
        Models::default()
    }

    /// The names the wire answered with. The newest listing wins, because it is the only
    /// one the pair in force produced: a listing that lands after the screen was closed
    /// and opened again names the wire as it is now.
    pub fn served(&mut self, listing: Listing) {
        self.listing = Some(listing);
        self.selected = 0;
    }

    /// Whether the wire has been asked and has not answered yet.
    pub fn waiting(&self) -> bool {
        self.listing.is_none()
    }

    /// The one line a wire that could not answer left: what the screen says where the
    /// names would go, and nothing at all when the wire answered (§10).
    pub fn hint(&self) -> Option<&str> {
        self.listing.as_ref().and_then(Listing::hint)
    }

    /// The query as it stands.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The rows the screen paints: the names the query leaves, best match first, at most
    /// [`CAP`] of them. A name the query leaves out is not a row at all, so narrowing the
    /// query is the only way to reach a name past the cap.
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
        // The rank decides, and the wire's own order decides what rank cannot: the sort is
        // stable, so two names that match as well as each other stay in the order the wire
        // listed them in.
        ranked.sort_by_key(|(rank, _)| *rank);
        ranked.into_iter().take(CAP).map(|(_, row)| row).collect()
    }

    /// Which of the offered rows the selection is on.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The filter box's own map: a glyph widens the query, a backspace narrows it, and a
    /// key a filter box has no use for moves nothing. Says whether anything changed.
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

    /// `↑`: the row above the selection, if there is one.
    pub fn up(&mut self) -> bool {
        let moved = self.selected > 0;
        self.selected = self.selected.saturating_sub(1);
        moved
    }

    /// `↓`: the row below the selection, if there is one.
    pub fn down(&mut self) -> bool {
        let moved = self.selected.saturating_add(1) < self.rows().len();
        if moved {
            self.selected += 1;
        }
        moved
    }

    /// The name under the selection: the identifier `Enter` writes, and `None` while the
    /// wire has not answered or the query left no row to take.
    pub fn chosen(&self) -> Option<&str> {
        self.rows().get(self.selected).map(|(id, _)| *id)
    }
}

/// How well one served name answers a query, lowest first: a name that starts with the
/// query above one that merely holds it, and nothing at all for a name the query leaves
/// out.
///
/// It is a ranking and not a filter dressed up, because a reader typing `haiku` means the
/// haiku model and not every name with those glyphs in the middle: the two ranks are the
/// whole of "ranked as you type", and the wire's order settles the rest.
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
