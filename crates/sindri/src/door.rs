//! The front door: Sindri's face over the wordmark, the one line that says what the product is, and
//! three commands with the line that explains each, set as one block and centred in the band an
//! answer would go. The launch's own paint pass and never an entry, so a quit there leaves no record.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::commands;
use crate::face;
use crate::ink::{Face, Ink, Look};
use crate::link::Links;
use crate::palette::Palette;
use crate::pen;
use crate::picture::Surface;
use crate::pictures::Pictures;
use crate::rows::{Row, wrap_under};
use crate::size::Sizes;

const WORDMARK: &str = "sindri";

const WHAT: &str = "An AI agent that is a Unix filter and a terminal UI over one core.";

/// The columns between a command and the line that explains it: enough that the three lines start
/// in one column whatever the width of the command they hang from.
const HANG: u16 = 2;

/// The row kept blank between the face and the wordmark, so the wordmark is never read as part of
/// the picture.
const GAP: u16 = 1;

/// Paints the door into the band the transcript is drawn in: the block of words centred in it and
/// the face over them. A band shorter than the block draws it from the band's first row, so a
/// terminal too small holds the door's head rather than its middle.
pub fn paint(
    area: Rect,
    buffer: &mut Buffer,
    palette: &Palette,
    links: &mut Links,
    sizes: &mut Sizes,
    pictures: &mut Pictures,
    surface: &Surface,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = block(area.width);
    // A block wider than the band would be drawn over the rail beside it, so it gives way at the
    // band's edge: a frame too narrow for a glyph of its words holds the head of them.
    let columns = block
        .iter()
        .map(Row::width)
        .max()
        .unwrap_or(0)
        .min(area.width);
    let lines = u16::try_from(block.len()).unwrap_or(u16::MAX);
    let face = face::over_wordmark(
        columns,
        area.height.saturating_sub(lines.saturating_add(GAP)),
        surface,
    );
    let reserved = face
        .as_ref()
        .map_or(0, |picture| picture.rows().saturating_add(GAP));
    let height = lines.saturating_add(reserved);
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(columns) / 2);
    let mut y = area
        .y
        .saturating_add(area.height.saturating_sub(height) / 2);
    if let Some(face) = &face {
        pictures.place(
            face,
            surface.cell(),
            Rect {
                x,
                y,
                width: face.cols(),
                height: face.rows(),
            },
            buffer,
        );
        y = y.saturating_add(reserved);
    }
    for row in block {
        if y >= area.bottom() {
            return;
        }
        row.paint(
            Rect {
                x,
                y,
                width: columns,
                height: 1,
            },
            buffer,
            palette,
            links,
            sizes,
        );
        y = y.saturating_add(1);
    }
}

/// The door's rows as `room` columns are drawn of them, top to bottom.
fn block(room: u16) -> Vec<Row> {
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
    rows.extend(commands(room));
    rows
}

/// The three commands as `room` columns are drawn of them: each hanging from its own column with
/// the line that explains it, and — where the band leaves that line less than the command's own
/// width — the two set on rows of their own, so no word of either is cut at the edge.
fn commands(room: u16) -> Vec<Row> {
    let named = || commands::LIST.iter().take(commands::NAMED);
    let column = named().map(|said| pen::cells(said.name)).max().unwrap_or(0) + HANG;
    let hang = pen::blank(column);
    let mut rows = Vec::new();
    for said in named() {
        if room < column.saturating_mul(2) {
            rows.push(Row::of([(
                String::from(said.name),
                Look::plain(Ink::Accent),
            )]));
            rows.extend(
                wrap_under(said.says, room, room)
                    .into_iter()
                    .map(|line| Row::of([(line, Look::plain(Ink::Dim))])),
            );
            continue;
        }
        let hanging = room - column;
        let mut lines = wrap_under(said.says, hanging, hanging).into_iter();
        if let Some(first) = lines.next() {
            rows.push(Row::of([
                (pen::pad(said.name, column), Look::plain(Ink::Accent)),
                (first, Look::plain(Ink::Dim)),
            ]));
        }
        rows.extend(lines.map(|line| Row::of([(format!("{hang}{line}"), Look::plain(Ink::Dim))])));
    }
    rows
}
