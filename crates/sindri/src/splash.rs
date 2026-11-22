use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::commands;
use crate::ink::{Face, Ink, Look};
use crate::link::Links;
use crate::palette::Palette;
use crate::pen;
use crate::rows::{Row, wrap_under};
use crate::size::Sizes;

const WORDMARK: &str = "sindri";

const WHAT: &str = "An AI agent that is a Unix filter and a terminal UI over one core.";

const HANG: u16 = 2;

pub fn paint(
    area: Rect,
    buffer: &mut Buffer,
    palette: &Palette,
    links: &mut Links,
    sizes: &mut Sizes,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let block = block(area.width);
    let columns = block
        .iter()
        .map(Row::width)
        .max()
        .unwrap_or(0)
        .min(area.width);
    let lines = u16::try_from(block.len()).unwrap_or(u16::MAX);
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(columns) / 2);
    let mut y = area.y.saturating_add(area.height.saturating_sub(lines) / 2);
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

fn commands(room: u16) -> Vec<Row> {
    let named = || commands::TABLE.iter().take(commands::SPLASH);
    let column = named().map(|row| pen::cells(row.name)).max().unwrap_or(0) + HANG;
    let hang = pen::blank(column);
    let mut rows = Vec::new();
    for row in named() {
        if room < column.saturating_mul(2) {
            rows.push(Row::of([(
                String::from(row.name),
                Look::plain(Ink::Accent),
            )]));
            rows.extend(
                wrap_under(row.says, room, room)
                    .into_iter()
                    .map(|line| Row::of([(line, Look::plain(Ink::Dim))])),
            );
            continue;
        }
        let hanging = room - column;
        let mut lines = wrap_under(row.says, hanging, hanging).into_iter();
        if let Some(first) = lines.next() {
            rows.push(Row::of([
                (pen::pad(row.name, column), Look::plain(Ink::Accent)),
                (first, Look::plain(Ink::Dim)),
            ]));
        }
        rows.extend(lines.map(|line| Row::of([(format!("{hang}{line}"), Look::plain(Ink::Dim))])));
    }
    rows
}
