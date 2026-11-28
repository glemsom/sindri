use crate::ink::{Ink, Look};
use crate::pen::{self, Clip};
use crate::rows::wrap;

const GAP: u16 = 2;

pub const QUERY: &str = "query: ";

pub const MARK: &str = "\u{203a} ";

pub struct ScreenRow {
    pub spans: Vec<(String, Look)>,
    pub clip: Clip,
    pub choice: Option<usize>,
    pub caret: Option<u16>,
}

impl ScreenRow {
    pub fn whole(spans: Vec<(String, Look)>) -> ScreenRow {
        ScreenRow {
            spans,
            clip: Clip::Whole,
            choice: None,
            caret: None,
        }
    }

    pub fn cut(spans: Vec<(String, Look)>) -> ScreenRow {
        ScreenRow {
            spans,
            clip: Clip::Edge,
            choice: None,
            caret: None,
        }
    }

    pub fn choosing(self, at: usize) -> ScreenRow {
        ScreenRow {
            choice: Some(at),
            ..self
        }
    }

    pub fn holding(self, column: u16) -> ScreenRow {
        ScreenRow {
            caret: Some(column),
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beside {
    Own(Ink),
    Row,
}

pub struct Item {
    pub name: String,
    pub beside: Vec<(String, Beside)>,
    pub ink: Ink,
}

impl Item {
    pub fn new(name: impl Into<String>) -> Item {
        Item {
            name: name.into(),
            beside: Vec::new(),
            ink: Ink::Plain,
        }
    }

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

fn widest<'a>(names: impl Iterator<Item = &'a str>) -> u16 {
    names.map(|name| pen::cells(name) + GAP).max().unwrap_or(0)
}

pub struct Column {
    columns: u16,
    width: u16,
    name: Look,
    said: Look,
}

impl Column {
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

    pub fn hung(&self, name: &str, said: &str) -> Vec<ScreenRow> {
        self.hung_in(name, self.name, said)
    }

    pub fn hung_in(&self, name: &str, name_look: Look, said: &str) -> Vec<ScreenRow> {
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
                ScreenRow::whole(vec![(lead, name_look), (line, self.said)])
            })
            .collect()
    }
}

pub fn whole(words: &str, look: Look) -> ScreenRow {
    ScreenRow::whole(vec![(words.to_owned(), look)])
}

pub fn refusal(words: &str, width: u16) -> Vec<ScreenRow> {
    let mut rows = vec![ScreenRow::whole(Vec::new())];
    for line in wrap(words, width) {
        rows.push(ScreenRow::cut(vec![(line, Look::plain(Ink::Err))]));
    }
    rows
}

pub struct List<'a> {
    pub query: Option<&'a str>,
    pub said: Option<&'a str>,
    pub items: Vec<Item>,
    pub selected: usize,
    pub marker: &'a str,
    pub chooses: bool,
}

impl List<'_> {
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
