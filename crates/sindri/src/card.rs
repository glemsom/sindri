//! The card every screen is drawn on: a rounded border carrying the screen's name in its top row
//! and the key that leaves in its bottom row, over the frame it was opened from, dimmed.

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen::{self, Clip};

/// The widest a card is drawn: the columns past it are the scrim's.
const WIDTH: u16 = 72;

/// The columns a card spends before its body begins: a border and a column of inset.
const INSET: u16 = 2;

/// The narrowest frame a card is drawn in: under it the border spends more columns than it gives
/// back, so the card bleeds to the frame's own edges instead.
const NARROW: u16 = 50;

/// The columns the key that leaves keeps clear at each end of the bottom row: the corner's own
/// column and one of rule.
const LEAVES_INSET: u16 = 2;

const TOP: (char, char) = ('╭', '╮');

const BOTTOM: (char, char) = ('╰', '╯');

const RULE: char = '─';

const SIDE: char = '│';

/// Paints a card named `name`, whose key that leaves is `leaves`, into `area`, and returns the band
/// the screen inside it draws in: the card's columns less its border and inset, and its rows less
/// its border.
pub fn paint(area: Rect, name: &str, leaves: &str, buffer: &mut Buffer, palette: &Palette) -> Rect {
    if area.width < NARROW {
        return bleeding(area, name, buffer, palette);
    }
    // A card that is the whole frame has no gutter left to grey, and a scrim painted under it
    // would be covered in the same pass.
    scrim(area, buffer, palette);
    boxed(area, name, leaves, buffer, palette)
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

fn boxed(area: Rect, name: &str, leaves: &str, buffer: &mut Buffer, palette: &Palette) -> Rect {
    let width = area.width.min(WIDTH);
    let card = Rect {
        x: area.x + (area.width - width) / 2,
        width,
        ..area
    };
    grounded(buffer, card, palette);

    let last = card.bottom() - 1;
    pen::rule(buffer, Rect { height: 1, ..card }, RULE, Ink::Dim, palette);
    headed(buffer, card, name, palette);
    if area.height > 1 {
        for y in card.y + 1..last {
            sides(buffer, card, y, palette);
        }
        footed(buffer, card, leaves, palette);
    }

    Rect {
        x: card.x + INSET,
        y: card.y + 1,
        width: width.saturating_sub(INSET * 2),
        height: area.height.saturating_sub(2),
    }
}

/// The card a frame too narrow or too short to hold one is drawn as: the title row, and the body it
/// heads at the card's own inset, with no border to spend columns and rows on.
fn bleeding(area: Rect, name: &str, buffer: &mut Buffer, palette: &Palette) -> Rect {
    grounded(buffer, area, palette);
    let head = Rect { height: 1, ..area };
    pen::rule(buffer, head, RULE, Ink::Dim, palette);
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
    corners(buffer, card, card.y, TOP, palette);
}

fn corners(
    buffer: &mut Buffer,
    card: Rect,
    y: u16,
    (left, right): (char, char),
    palette: &Palette,
) {
    pen::glyph(buffer, card.x, y, left, Ink::Dim, palette);
    pen::glyph(buffer, card.right() - 1, y, right, Ink::Dim, palette);
}

fn sides(buffer: &mut Buffer, card: Rect, y: u16, palette: &Palette) {
    for x in [card.x, card.right() - 1] {
        pen::glyph(buffer, x, y, SIDE, Ink::Dim, palette);
    }
}

/// The key that leaves hangs in the bottom row as the name hangs in the top one, at the other end.
fn footed(buffer: &mut Buffer, card: Rect, leaves: &str, palette: &Palette) {
    let row = Rect {
        y: card.bottom() - 1,
        height: 1,
        ..card
    };
    pen::rule(buffer, row, RULE, Ink::Dim, palette);
    corners(buffer, card, row.y, BOTTOM, palette);
    pen::hang(
        buffer,
        Rect {
            x: card.x + LEAVES_INSET,
            width: card.width.saturating_sub(LEAVES_INSET * 2),
            ..row
        },
        &[(" ", Ink::Dim), (leaves, Ink::Muted), (" ", Ink::Dim)],
        palette,
    );
}

fn labelled(buffer: &mut Buffer, row: Rect, name: &str, palette: &Palette) {
    if name.is_empty() {
        return;
    }
    pen::write(
        buffer,
        Rect {
            x: row.x + INSET,
            width: row.width.saturating_sub(INSET + 1),
            height: 1,
            ..row
        },
        &[(" ", Ink::Dim), (name, Ink::Accent), (" ", Ink::Dim)],
        Clip::Edge,
        palette,
    );
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
