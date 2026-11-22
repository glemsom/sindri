use ratatui::buffer::Buffer;
use ratatui::style::Color;

use crate::ink::Ink;
use crate::palette::Palette;
use crate::pen;

pub(crate) fn inks(buffer: &Buffer, y: u16, palette: &Palette) -> Vec<(Ink, u16)> {
    let mut runs: Vec<(Ink, u16)> = Vec::new();
    for x in buffer.area.x..buffer.area.right() {
        let cell = buffer
            .cell((x, y))
            .unwrap_or_else(|| panic!("row {y}, column {x} is outside the band"));
        let ink = Ink::of(cell, palette).expect("every cell of a band is drawn in an ink");
        match runs.last_mut() {
            Some((last, count)) if *last == ink => *count += 1,
            _ => runs.push((ink, 1)),
        }
    }
    runs
}

pub(crate) fn under(buffer: &Buffer, x: u16, y: u16) -> Color {
    buffer
        .cell((x, y))
        .unwrap_or_else(|| panic!("row {y}, column {x} is outside the band"))
        .bg
}

pub(crate) fn column_of(row: &str, glyph: char) -> Option<u16> {
    let at = row.find(glyph)?;
    Some(pen::cells(&row[..at]))
}
