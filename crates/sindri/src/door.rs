//! The front door: the wordmark, the one line that says what the product is, and three commands
//! with the line that explains each, laid in the transcript band where an answer would go. It is
//! the launch's own paint pass and never an entry, so a quit at the door leaves no record.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ink::{Face, Ink, Look};
use crate::palette::Palette;
use crate::rows::{Row, wrap_under};

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

/// Paints the door into the band the transcript is drawn in, indented as an answer is. A line
/// wider than the band wraps under its own column, so a narrow frame is never shown half a word.
pub fn paint(area: Rect, buffer: &mut Buffer, palette: &Palette) {
    let room = area.width.saturating_sub(INDENT);
    if room == 0 {
        return;
    }
    let area = Rect {
        x: area.x.saturating_add(INDENT),
        width: area.width.saturating_sub(INDENT),
        ..area
    };
    let mut y = area.y;
    for row in rows(room) {
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

/// The door's rows as `room` columns are drawn of them, top to bottom.
fn rows(room: u16) -> Vec<Row> {
    let mut rows = vec![
        Row::of([(String::from(WORDMARK), Look::new(Ink::Accent, Face::Bold))]),
        Row::blank(),
    ];
    rows.extend(
        wrap_under(WHAT, room, room)
            .into_iter()
            .map(|line| Row::of([(line, Look::plain(Ink::Plain))])),
    );
    rows.push(Row::blank());

    let column = COMMANDS
        .iter()
        .map(|(command, _)| command.chars().count())
        .max()
        .unwrap_or(0)
        + HANG;
    for (command, said) in COMMANDS {
        let hanging = usize::from(room).saturating_sub(column);
        let hanging = u16::try_from(hanging).unwrap_or(room);
        let mut lines = wrap_under(said, hanging, hanging).into_iter();
        if let Some(first) = lines.next() {
            rows.push(Row::of([
                (format!("{command:<column$}"), Look::plain(Ink::Accent)),
                (first, Look::plain(Ink::Dim)),
            ]));
        }
        rows.extend(lines.map(|line| {
            Row::of([(
                format!("{}{line}", " ".repeat(column)),
                Look::plain(Ink::Dim),
            )])
        }));
    }
    rows
}
