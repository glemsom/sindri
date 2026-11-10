//! The card every screen is drawn on: a rounded border carrying the screen's name in its top row
//! and the key that leaves in its bottom row, over the frame it was opened from, dimmed.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use crate::ink::Ink;
use crate::palette::Palette;

/// The widest a card is drawn: the columns past it are the scrim's.
const WIDTH: u16 = 72;

/// What a card costs the frame it stands in: a border and a column of inset on each side.
const CHROME: u16 = 4;

/// The narrowest frame a card is drawn in, and the shortest: under either, the border says less
/// than the columns and rows it costs, so the card bleeds to the frame's own edges.
const FLOOR: (u16, u16) = (50, 3);

/// The key every screen leaves on.
const LEAVE: &str = "esc";

const CORNERS: [char; 4] = ['╭', '╮', '╰', '╯'];

const RULE: char = '─';

const SIDE: char = '│';

/// Paints a card named `name` into `area` and returns the band the screen inside it draws in: the
/// card's columns less its border and inset, and its rows less its border.
pub fn paint(area: Rect, name: &str, buffer: &mut Buffer, palette: &Palette) -> Rect {
    scrim(area, buffer, palette);
    match area.width < FLOOR.0 || area.height < FLOOR.1 {
        true => bleeding(area, name, buffer, palette),
        false => boxed(area, name, buffer, palette),
    }
}

/// Dims every cell of the frame a card is opened from. A background is kept, so the band a
/// greyed cell sits on is still that band, and a look that spends a modifier gives it up: a
/// scrimmed frame is read and not acted on.
fn scrim(area: Rect, buffer: &mut Buffer, palette: &Palette) {
    let (foreground, _) = palette.ink(Ink::Dim);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            cell.set_fg(foreground);
            cell.modifier = Modifier::empty();
        }
    }
}

/// A card's own cells start at the frame's ground: neither a band nor a transcript behind it is
/// read through the card that covers it.
fn grounded(buffer: &mut Buffer, rect: Rect, palette: &Palette) {
    let style = palette.ground(None);
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let Some(cell) = buffer.cell_mut((x, y)) else {
                continue;
            };
            cell.reset();
            cell.set_style(style);
        }
    }
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
    filled(buffer, card, RULE, palette);
    for y in card.y + 1..last {
        put(buffer, card.x, y, SIDE, Ink::Dim, palette);
        put(buffer, card.right() - 1, y, SIDE, Ink::Dim, palette);
    }
    headed(buffer, card, name, palette);
    footed(buffer, card, palette);

    Rect {
        x: card.x + 2,
        y: card.y + 1,
        width: width.saturating_sub(CHROME),
        height: area.height - 2,
    }
}

/// The card a frame too narrow or too short to hold one is drawn as: the title row and the body it
/// heads, at the frame's own edges, and no border to spend columns on.
fn bleeding(area: Rect, name: &str, buffer: &mut Buffer, palette: &Palette) -> Rect {
    grounded(buffer, area, palette);
    let head = Rect { height: 1, ..area };
    filled(buffer, head, RULE, palette);
    labelled(buffer, head, name, palette);
    Rect {
        y: area.y.saturating_add(1),
        height: area.height.saturating_sub(1),
        ..area
    }
}

/// The top row: the frame's rule with the screen's name hung in it, its corners rounded.
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

/// The bottom row: the frame's rule with the key that leaves hung in it, its corners rounded.
fn footed(buffer: &mut Buffer, card: Rect, palette: &Palette) {
    let (y, right) = (card.bottom() - 1, card.right());
    filled(
        buffer,
        Rect {
            y,
            height: 1,
            ..card
        },
        RULE,
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

/// Hangs a name in a row of rules: a column of rule, the name, and the column after it.
fn labelled(buffer: &mut Buffer, row: Rect, name: &str, palette: &Palette) {
    let room = row.width.saturating_sub(CHROME + 1);
    let name: String = name.chars().take(usize::from(room)).collect();
    if name.is_empty() {
        return;
    }
    let mut x = row.x + 2;
    put(buffer, x, row.y, ' ', Ink::Dim, palette);
    x += 1;
    for glyph in name.chars() {
        put(buffer, x, row.y, glyph, Ink::Accent, palette);
        x += 1;
    }
    put(buffer, x, row.y, ' ', Ink::Dim, palette);
}

fn filled(buffer: &mut Buffer, row: Rect, glyph: char, palette: &Palette) {
    for x in row.x..row.right() {
        put(buffer, x, row.y, glyph, Ink::Dim, palette);
    }
}

fn put(buffer: &mut Buffer, x: u16, y: u16, glyph: char, ink: Ink, palette: &Palette) {
    if let Some(cell) = buffer.cell_mut((x, y)) {
        cell.set_char(glyph);
        cell.set_style(ink.style(palette));
    }
}
