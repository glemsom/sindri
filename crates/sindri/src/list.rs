//! The list a card draws: the query it answers at its head, the one line it says in place of its
//! rows, and one row per item with the row a reader is on marked — each filled down the band a card
//! hands it and cut at the frame's edge — and, under the rows, the refusal that turned a visit back.

use crate::ink::{Ink, Look};
use crate::pen::{self, Clip};
use crate::rows::wrap;

/// The columns a row keeps beside a name, before the words that stand beside it.
const GAP: u16 = 2;

/// The words a list's query is typed after.
pub const QUERY: &str = "query: ";

/// The glyph before the row a reader is on, in every list a card draws: one mark, because every
/// screen that lists says the same thing with it.
pub const MARK: &str = "\u{203a} ";

/// One row of a card as data: the spans it draws, each in its own look, and the rule that decides
/// what happens to a row the card has no room for.
pub struct ScreenRow {
    pub spans: Vec<(String, Look)>,
    pub clip: Clip,
    /// The place this row holds in the list its screen keeps, or nothing where no key hangs from it.
    pub choice: Option<usize>,
    /// The column of this row, in cells from its first one, where the caret waits, and nothing for a
    /// row no glyph is typed at.
    pub caret: Option<u16>,
}

impl ScreenRow {
    /// A row whole or not there at all.
    pub fn whole(spans: Vec<(String, Look)>) -> ScreenRow {
        ScreenRow {
            spans,
            clip: Clip::Whole,
            choice: None,
            caret: None,
        }
    }

    /// A row cut at its right edge: a value that runs on is cut rather than dropped.
    pub fn cut(spans: Vec<(String, Look)>) -> ScreenRow {
        ScreenRow {
            spans,
            clip: Clip::Edge,
            choice: None,
            caret: None,
        }
    }

    /// The same row, marked as the `at`th of the list its screen keeps: a press on it lands there.
    pub fn choosing(self, at: usize) -> ScreenRow {
        ScreenRow {
            choice: Some(at),
            ..self
        }
    }

    /// The same row, holding the cell the caret waits in `column` cells in from its first one.
    pub fn holding(self, column: u16) -> ScreenRow {
        ScreenRow {
            caret: Some(column),
            ..self
        }
    }
}

/// The ink the words beside a name are read in: one of their own, for words that describe the name,
/// or the row's own, for a value it holds — plain while the reader is elsewhere, and the accent on
/// the row the reader is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beside {
    Own(Ink),
    Row,
}

/// One item of a list: the words a reader is looking for, and the words that stand beside them,
/// each in its own ink.
pub struct Item {
    pub name: String,
    pub beside: Vec<(String, Beside)>,
    /// The ink the name and the marker's blank columns stand in when the reader is not on the row,
    /// so a screen can list keys quieter than the values beside them.
    pub ink: Ink,
}

impl Item {
    /// An item that is only its name: the words that stand beside it are the screen's to add.
    pub fn new(name: impl Into<String>) -> Item {
        Item {
            name: name.into(),
            beside: Vec::new(),
            ink: Ink::Plain,
        }
    }

    /// An item whose name says what it is and whose words beside it say the rest: the gap between
    /// them, and those words set in the quieter ink.
    pub fn beside(name: impl Into<String>, said: impl Into<String>) -> Item {
        Item {
            name: name.into(),
            beside: vec![(
                format!("{}{}", pen::blank(GAP), said.into()),
                Beside::Own(Ink::Dim),
            )],
            ink: Ink::Plain,
        }
    }
}

/// The items a column of names makes: each name padded to the widest of them and the gap beside it,
/// so the words beside them begin in one column — the names in `name_ink`, and the words beside each
/// of them in the ink `beside` names.
pub fn columned<'a>(
    rows: impl Iterator<Item = (&'a str, &'a str)>,
    name_ink: Ink,
    beside: Beside,
) -> Vec<Item> {
    let rows: Vec<(&str, &str)> = rows.collect();
    let widest = widest(rows.iter().map(|(name, _)| *name));
    rows.into_iter()
        .map(|(name, said)| Item {
            name: pen::pad(name, widest),
            beside: vec![(said.to_owned(), beside)],
            ink: name_ink,
        })
        .collect()
}

/// The columns the widest of `names` takes beside the gap after it: the one column a list of names
/// keeps, whether it is read as items or drawn beside the lines that explain them.
fn widest<'a>(names: impl Iterator<Item = &'a str>) -> u16 {
    names.map(|name| pen::cells(name) + GAP).max().unwrap_or(0)
}

/// The column the names of a list keep, and the band the words beside them are wrapped to: the
/// columns a name is padded to, the width a line is wrapped to, and the two looks each is read in.
pub struct Column {
    columns: u16,
    width: u16,
    name: Look,
    said: Look,
}

impl Column {
    /// The column the `names` of a list keep: the widest of them and the gap after it, in a band
    /// `width` columns wide, the names read in `name` and the words beside them in `said`.
    pub fn of<'a>(
        names: impl Iterator<Item = &'a str>,
        width: u16,
        name: Look,
        said: Look,
    ) -> Column {
        Column {
            columns: widest(names),
            width,
            name,
            said,
        }
    }

    /// The rows one name and the line that explains it make: the name in the column's own columns,
    /// its line wrapped under that column, and every row after the first hanging at it.
    pub fn hung(&self, name: &str, said: &str) -> Vec<ScreenRow> {
        let hang = pen::blank(self.columns);
        let mut lines = wrap(said, self.width.saturating_sub(self.columns));
        if lines.is_empty() {
            lines.push(String::new());
        }
        lines
            .into_iter()
            .enumerate()
            .map(|(at, line)| {
                let lead = match at {
                    0 => pen::pad(name, self.columns),
                    _ => hang.clone(),
                };
                ScreenRow::whole(vec![(lead, self.name), (line, self.said)])
            })
            .collect()
    }
}

/// One row of a list that is words alone, in one look: whole, or not drawn at all.
pub fn whole(words: &str, look: Look) -> ScreenRow {
    ScreenRow::whole(vec![(words.to_owned(), look)])
}

/// The rows a refusal is read in under a list: a blank row, then its words wrapped to the band, each
/// line in the ink an error is read in and cut at the frame's edge.
pub fn refusal(words: &str, width: u16) -> Vec<ScreenRow> {
    let mut rows = vec![ScreenRow::whole(Vec::new())];
    for line in wrap(words, width) {
        rows.push(ScreenRow::cut(vec![(line, Look::plain(Ink::Err))]));
    }
    rows
}

/// What a list shows: the query it answers, the one line it says in place of its rows, one row per
/// item, and the row the reader is on.
pub struct List<'a> {
    pub query: Option<&'a str>,
    pub said: Option<&'a str>,
    pub items: Vec<Item>,
    pub selected: usize,
    pub marker: &'a str,
    /// Whether a press on its rows chooses them, which the screen keeping the list is the one to say.
    pub chooses: bool,
}

impl List<'_> {
    /// The rows this list fills its band with: the query at the head, the one line it says in place
    /// of its rows, or one row per item with the selected one marked — each cut at the frame's edge.
    pub fn rows(&self) -> Vec<ScreenRow> {
        let mut rows = Vec::new();
        if let Some(typed) = self.query {
            let spans = vec![
                (QUERY.to_owned(), Look::plain(Ink::Dim)),
                (typed.to_owned(), Look::plain(Ink::Muted)),
            ];
            let column = pen::width(&spans);
            rows.push(ScreenRow::cut(spans).holding(column));
        }
        if let Some(words) = self.said {
            rows.push(whole(words, Look::plain(Ink::Dim)));
            return rows;
        }
        for (at, item) in self.items.iter().enumerate() {
            let marked = at == self.selected;
            let (marker, named) = match marked {
                true => (self.marker.to_owned(), Ink::Accent),
                false => (pen::blank(pen::cells(self.marker)), item.ink),
            };
            let look = Look::plain(named);
            let mut spans = vec![(marker, look), (item.name.clone(), look)];
            spans.extend(item.beside.iter().map(|(said, beside)| {
                let ink = match beside {
                    Beside::Own(ink) => *ink,
                    Beside::Row if marked => Ink::Accent,
                    Beside::Row => Ink::Plain,
                };
                (said.clone(), Look::plain(ink))
            }));
            let mut row = ScreenRow::cut(spans);
            if self.chooses {
                row = row.choosing(at);
            }
            rows.push(row);
        }
        rows
    }
}

#[cfg(test)]
mod tests;
