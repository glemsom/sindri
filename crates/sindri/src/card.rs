//! The card every screen is drawn on: a rounded border carrying the screen's name in its top row
//! and the key that leaves in its bottom row, over the frame it was opened from, dimmed.

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use crate::ink::Ink;
use crate::palette::Palette;

/// The widest a card is drawn: the columns past it are the scrim's.
const WIDTH: u16 = 72;

/// The columns a card spends before its body begins: a border and a column of inset.
const INSET: u16 = 2;

/// The narrowest frame a card is drawn in: under it the border spends more columns than it gives
/// back, so the card bleeds to the frame's own edges instead.
const NARROW: u16 = 50;

/// The shortest: two borders and the one row between them.
const SHORT: u16 = 3;

const LEAVE: &str = "esc";

const CORNERS: [char; 4] = ['╭', '╮', '╰', '╯'];

const RULE: char = '─';

const SIDE: char = '│';

/// Paints a card named `name` into `area`, and returns the band the screen inside it draws in:
/// the card's columns less its border and inset, and its rows less its border.
pub fn paint(area: Rect, name: &str, buffer: &mut Buffer, palette: &Palette) -> Rect {
    if area.width < NARROW || area.height < SHORT {
        return bleeding(area, name, buffer, palette);
    }
    // A card that is the whole frame has no gutter left to grey, and a scrim painted under it
    // would be covered in the same pass.
    scrim(area, buffer, palette);
    boxed(area, name, buffer, palette)
}

/// Dims every cell of the frame a card is opened from, so no glyph behind it is read at the
/// strength the screen drew it. A cell keeps its background, because a band is the surface a cell
/// sits on and the grey says the words are out of reach, not that the band has gone.
fn scrim(area: Rect, buffer: &mut Buffer, palette: &Palette) {
    let (foreground, _) = palette.ink(Ink::Dim);
    each(area, buffer, |cell| {
        cell.set_fg(foreground);
        cell.modifier = Modifier::empty();
    });
}

/// A card's own cells start at the frame's ground: neither a band nor a transcript behind it is
/// read through the card that covers it.
fn grounded(buffer: &mut Buffer, rect: Rect, palette: &Palette) {
    let style = palette.ground(None);
    each(rect, buffer, |cell| {
        cell.reset();
        cell.set_style(style);
    });
}

fn boxed(area: Rect, name: &str, buffer: &mut Buffer, palette: &Palette) -> Rect {
    let width = area.width.min(WIDTH);
    let card = Rect {
        x: area.x + (area.width - width) / 2,
        width,
        ..area
    };
    grounded(buffer, card, palette);

    let last = card.bottom() - 1;
    ruled(buffer, Rect { height: 1, ..card }, palette);
    for y in card.y + 1..last {
        put(buffer, card.x, y, SIDE, Ink::Dim, palette);
        put(buffer, card.right() - 1, y, SIDE, Ink::Dim, palette);
    }
    headed(buffer, card, name, palette);
    footed(buffer, card, palette);

    Rect {
        x: card.x + INSET,
        y: card.y + 1,
        width: width.saturating_sub(INSET * 2),
        height: area.height - 2,
    }
}

/// The card a frame too narrow or too short to hold one is drawn as: the title row, and the body it
/// heads at the card's own inset, with no border to spend columns and rows on.
fn bleeding(area: Rect, name: &str, buffer: &mut Buffer, palette: &Palette) -> Rect {
    grounded(buffer, area, palette);
    let head = Rect { height: 1, ..area };
    ruled(buffer, head, palette);
    labelled(buffer, head, name, palette);
    Rect {
        x: area.x.saturating_add(INSET),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(INSET * 2),
        height: area.height.saturating_sub(1),
    }
}

fn headed(buffer: &mut Buffer, card: Rect, name: &str, palette: &Palette) {
    labelled(buffer, Rect { height: 1, ..card }, name, palette);
    put(buffer, card.x, card.y, CORNERS[0], Ink::Dim, palette);
    put(
        buffer,
        card.right() - 1,
        card.y,
        CORNERS[1],
        Ink::Dim,
        palette,
    );
}

/// The key that leaves hangs in the bottom row as the name hangs in the top one, at the other end.
fn footed(buffer: &mut Buffer, card: Rect, palette: &Palette) {
    let (y, right) = (card.bottom() - 1, card.right());
    ruled(
        buffer,
        Rect {
            y,
            height: 1,
            ..card
        },
        palette,
    );
    put(buffer, card.x, y, CORNERS[2], Ink::Dim, palette);
    put(buffer, right - 1, y, CORNERS[3], Ink::Dim, palette);

    let room = u16::try_from(LEAVE.chars().count()).unwrap_or(right);
    let mut x = right.saturating_sub(room + 4);
    if x <= card.x + 1 {
        return;
    }
    put(buffer, x, y, ' ', Ink::Dim, palette);
    x += 1;
    for glyph in LEAVE.chars() {
        put(buffer, x, y, glyph, Ink::Muted, palette);
        x += 1;
    }
    put(buffer, x, y, ' ', Ink::Dim, palette);
}

fn labelled(buffer: &mut Buffer, row: Rect, name: &str, palette: &Palette) {
    let room = row.width.saturating_sub(INSET * 2 + 1);
    let name: String = name.chars().take(usize::from(room)).collect();
    if name.is_empty() {
        return;
    }
    let mut x = row.x + INSET;
    put(buffer, x, row.y, ' ', Ink::Dim, palette);
    x += 1;
    for glyph in name.chars() {
        put(buffer, x, row.y, glyph, Ink::Accent, palette);
        x += 1;
    }
    put(buffer, x, row.y, ' ', Ink::Dim, palette);
}

fn ruled(buffer: &mut Buffer, row: Rect, palette: &Palette) {
    for x in row.x..row.right() {
        put(buffer, x, row.y, RULE, Ink::Dim, palette);
    }
}

fn each(area: Rect, buffer: &mut Buffer, mut edit: impl FnMut(&mut Cell)) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                edit(cell);
            }
        }
    }
}

fn put(buffer: &mut Buffer, x: u16, y: u16, glyph: char, ink: Ink, palette: &Palette) {
    if let Some(cell) = buffer.cell_mut((x, y)) {
        cell.set_char(glyph);
        cell.set_style(ink.style(palette));
    }
}
