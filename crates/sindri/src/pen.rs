//! The pen: the one place a run of glyphs is written into a frame, the one place a string's width is
//! measured — a wide glyph two cells wherever it is drawn, a narrow one one — and the one place a
//! row is filled to its edge or a run is hung on it.

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::ink::Look;
use crate::palette::Palette;

/// How much of a run a row has no room for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    /// A run the row has no room for is not drawn at all.
    Whole,
    /// A run is drawn up to the row's right edge and the rest is dropped.
    Edge,
}

/// The columns `text` takes on a row: a wide glyph two cells, a combining mark none.
pub fn cells(text: &str) -> u16 {
    u16::try_from(text.width()).unwrap_or(u16::MAX)
}

/// Whether `text` holds a glyph wider than one cell: the one case that hides the cell after it.
fn wide(text: &str) -> bool {
    text.chars().any(|glyph| cell(glyph) > 1)
}

/// How many glyphs a run is made of: a draft's bill counts these, and a column is never one of
/// them, since a column is the cells a run draws in.
pub fn glyphs(text: &str) -> usize {
    text.chars().count()
}

/// A count of columns as a row's own arithmetic carries it: one wider than any row can be drawn in
/// is the widest a row holds.
pub fn columns(columns: usize) -> u16 {
    u16::try_from(columns).unwrap_or(u16::MAX)
}

/// The columns one `glyph` takes, counted the way a row of it is drawn.
pub fn cell(glyph: char) -> u16 {
    cells(glyph.encode_utf8(&mut [0; 4]))
}

/// `columns` of one `glyph`: the run a row fills to its edge with, and the one a name is padded
/// out to.
pub fn filled(columns: u16, glyph: char) -> String {
    std::iter::repeat_n(glyph, usize::from(columns)).collect()
}

/// `columns` blanks: the run a value hangs from, and the one a name is padded out to.
pub fn blank(columns: u16) -> String {
    filled(columns, ' ')
}

/// `text` followed by the blanks that bring it out to `columns`, measured by what it draws. A run
/// already wider than its column is left whole, because the pen pads and never cuts.
pub fn pad(text: &str, columns: u16) -> String {
    let mut padded = String::from(text);
    padded.push_str(&blank(columns.saturating_sub(cells(text))));
    padded
}

/// Writes `spans` from `row`'s left edge, each in its own look, advancing by measured columns and
/// so leaving two behind every wide glyph.
pub fn write<S: AsRef<str>, D: Copy + Into<Look>>(
    buffer: &mut Buffer,
    row: Rect,
    spans: &[(S, D)],
    clip: Clip,
    palette: &Palette,
) {
    if clip == Clip::Whole && width(spans) > row.width {
        return;
    }
    let mut x = row.x;
    for (text, look) in spans {
        let look: Look = (*look).into();
        let room = usize::from(row.right().saturating_sub(x));
        let ground = buffer.cell((x, row.y)).map(|cell| cell.style());
        let (next, _) = buffer.set_stringn(x, row.y, text.as_ref(), room, look.style(palette));
        // `set_stringn` resets the cell a wide glyph hides, ground and all, which would read the band
        // the row sits on through the hole it leaves: the run is claimed in the look it was written
        // in, over the ground the cell had before it.
        if wide(text.as_ref()) {
            claim(
                buffer,
                Rect {
                    x,
                    y: row.y,
                    width: next.saturating_sub(x),
                    height: 1,
                },
                ground.unwrap_or_default().patch(look.style(palette)),
            );
        }
        x = next;
    }
}

/// Writes `text` from column `x` of row `y` and answers the column after it: the run is measured
/// by what it draws, so a wide glyph leaves the column behind it blank and is never cut in half.
pub fn run(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    text: &str,
    look: impl Into<Look>,
    palette: &Palette,
) -> u16 {
    let row = Rect {
        x,
        y,
        width: buffer.area.right().saturating_sub(x),
        height: 1,
    };
    write(buffer, row, &[(text, look.into())], Clip::Edge, palette);
    x.saturating_add(cells(text))
}

/// Writes a one-cell `glyph` at a column of a row, in a look.
pub fn glyph(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    glyph: char,
    look: impl Into<Look>,
    palette: &Palette,
) {
    let look: Look = look.into();
    each(Rect::new(x, y, 1, 1), buffer, |cell| {
        cell.set_char(glyph);
        cell.set_style(look.style(palette));
    });
}

/// Claims every cell of `area` in `style`, leaving each one its glyph, its own background and the
/// face it wore: a frame's ground, and a mark over a row of it.
pub fn claim(buffer: &mut Buffer, area: Rect, style: Style) {
    each(area, buffer, |cell| {
        cell.set_style(style);
    });
}

/// Claims every cell of `area` in `style` with every face it wore stripped, so a mark under a scrim
/// is read as the words it says and not as a mark.
pub fn wash(buffer: &mut Buffer, area: Rect, style: Style) {
    claim(buffer, area, style.remove_modifier(Modifier::all()));
}

/// Blanks every cell of `area` to `style`: the ground a card covers a frame with, so no glyph the
/// frame behind it drew is read through it.
pub fn ground(buffer: &mut Buffer, area: Rect, style: Style) {
    each(area, buffer, |cell| {
        cell.reset();
        cell.set_style(style);
    });
}

/// Stamps one cell in `style`, stripping every face and underline the glyph under it already wore so
/// the cell reads back as the style alone, and keeping the glyph and the ground it sits on.
pub fn stamp(buffer: &mut Buffer, x: u16, y: u16, style: Style) {
    each(Rect::new(x, y, 1, 1), buffer, |cell| {
        cell.modifier = Modifier::empty();
        cell.underline_color = Color::Reset;
        cell.set_style(style);
    });
}

/// Every cell of `area`, top to bottom and left to right.
fn each(area: Rect, buffer: &mut Buffer, mut edit: impl FnMut(&mut Cell)) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                edit(cell);
            }
        }
    }
}

/// Fills `row` edge to edge with a one-cell `fill`, in a look.
pub fn rule(buffer: &mut Buffer, row: Rect, fill: char, look: impl Into<Look>, palette: &Palette) {
    write(
        buffer,
        row,
        &[(filled(row.width, fill), look.into())],
        Clip::Edge,
        palette,
    );
}

/// The column a run `width` columns wide starts at when it hangs on `row`'s far edge, and none when
/// the row has no room for it: a fact's value and a card's key hang from this one answer.
pub fn hanging(row: Rect, width: u16) -> Option<u16> {
    (width <= row.width).then(|| row.right() - width)
}

/// Hangs `spans` on `row`'s far edge, so the last cell of the run is the row's last: a run the row
/// has no room for is refused whole and hangs nowhere.
pub fn hang<S: AsRef<str>, D: Copy + Into<Look>>(
    buffer: &mut Buffer,
    row: Rect,
    spans: &[(S, D)],
    palette: &Palette,
) {
    let Some(x) = hanging(row, width(spans)) else {
        return;
    };
    write(buffer, Rect { x, ..row }, spans, Clip::Edge, palette);
}

/// The columns a run of spans takes: every span measured by what it draws.
pub fn width<S: AsRef<str>, D>(spans: &[(S, D)]) -> u16 {
    spans
        .iter()
        .map(|(text, _)| cells(text.as_ref()))
        .fold(0, u16::saturating_add)
}

#[cfg(test)]
mod tests;
