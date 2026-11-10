//! The front door: the wordmark, the one line that says what the product is, and three commands
//! with the line that explains each, laid in the transcript band where an answer would go. It is
//! the launch's own paint pass and never an entry, so a quit at the door leaves no record.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::{Face, Ink, Look};
use crate::palette::Palette;
use crate::rows::Row;

/// The columns the door is set from: the gutter an answer is set in.
const INDENT: u16 = 2;

const WORDMARK: &str = "sindri";

const WHAT: &str = "An AI agent that is a Unix filter and a terminal UI over one core.";

/// The commands a reader can open with, and the one line each that says what it does.
const COMMANDS: [(&str, &str); 3] = [
    (
        "/model",
        "ask the wire what it serves, and name the one to run",
    ),
    (
        "/settings",
        "the keys the config holds, and the value each is set to",
    ),
    ("/stats", "the facts the rail is too narrow to hold"),
];

/// The columns between a command and the line that explains it: enough that the three lines start
/// in one column whatever the width of the command they hang from.
const HANG: usize = 2;

/// The rows a door is drawn as, in the order it reads them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Door {
    rows: Vec<Row>,
}

impl Default for Door {
    fn default() -> Door {
        Door::new()
    }
}

impl Door {
    pub fn new() -> Door {
        let field = COMMANDS
            .iter()
            .map(|(command, _)| command.chars().count())
            .max()
            .unwrap_or(0)
            + HANG;
        let mut rows = vec![
            Row::of([(String::from(WORDMARK), Look::new(Ink::Accent, Face::Bold))]),
            Row::blank(),
            Row::of([(String::from(WHAT), Look::plain(Ink::Plain))]),
            Row::blank(),
        ];
        rows.extend(COMMANDS.iter().map(|(command, said)| {
            Row::of([
                (format!("{command:<field$}"), Look::plain(Ink::Accent)),
                (String::from(*said), Look::plain(Ink::Dim)),
            ])
        }));
        Door { rows }
    }

    /// Paints the door into the band the transcript is drawn in, indented as an answer is. A band
    /// with fewer rows than the door draws the rows it holds, because an empty band says nothing.
    pub fn paint(&self, area: Rect, buffer: &mut Buffer, palette: &Palette) {
        let area = Rect {
            x: area.x.saturating_add(INDENT),
            width: area.width.saturating_sub(INDENT),
            ..area
        };
        let mut y = area.y;
        for row in &self.rows {
            if y >= area.bottom() {
                return;
            }
            row.paint(
                Rect {
                    height: 1,
                    y,
                    ..area
                },
                buffer,
                palette,
            );
            y = y.saturating_add(1);
        }
    }
}
