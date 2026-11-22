use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::ink::Look;
use crate::palette::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    Whole,
    Edge,
}

/// The columns `text` takes on a row: a wide glyph two cells, a combining mark none.
pub fn cells(text: &str) -> u16 {
    u16::try_from(text.width()).unwrap_or(u16::MAX)
}

fn wide(text: &str) -> bool {
    text.chars().any(|glyph| cell(glyph) > 1)
}

pub fn columns(columns: usize) -> u16 {
    u16::try_from(columns).unwrap_or(u16::MAX)
}

pub fn cell(glyph: char) -> u16 {
    cells(glyph.encode_utf8(&mut [0; 4]))
}

pub fn filled(columns: u16, glyph: char) -> String {
    std::iter::repeat_n(glyph, usize::from(columns)).collect()
}

pub fn blank(columns: u16) -> String {
    filled(columns, ' ')
}

pub fn pad(text: &str, columns: u16) -> String {
    let mut padded = String::from(text);
    padded.push_str(&blank(columns.saturating_sub(cells(text))));
    padded
}

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

pub fn claim(buffer: &mut Buffer, area: Rect, style: Style) {
    each(area, buffer, |cell| {
        cell.set_style(style);
    });
}

pub fn wash(buffer: &mut Buffer, area: Rect, style: Style) {
    claim(buffer, area, style.remove_modifier(Modifier::all()));
}

pub fn ground(buffer: &mut Buffer, area: Rect, style: Style) {
    each(area, buffer, |cell| {
        cell.reset();
        cell.set_style(style);
    });
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

pub fn rule(buffer: &mut Buffer, row: Rect, fill: char, look: impl Into<Look>, palette: &Palette) {
    write(
        buffer,
        row,
        &[(filled(row.width, fill), look.into())],
        Clip::Edge,
        palette,
    );
}

pub fn hanging(row: Rect, width: u16) -> Option<u16> {
    (width <= row.width).then(|| row.right() - width)
}

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

pub fn width<S: AsRef<str>, D>(spans: &[(S, D)]) -> u16 {
    spans
        .iter()
        .map(|(text, _)| cells(text.as_ref()))
        .fold(0, u16::saturating_add)
}

#[cfg(test)]
mod tests;
