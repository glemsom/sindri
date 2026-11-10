//! What a painted buffer holds, read back the way a test asks about it: the ink runs of one row,
//! and the column a glyph sits in.

use ratatui::buffer::Buffer;

use crate::ink::Ink;
use crate::palette::Palette;

/// The ink of each run of cells along `y`, left to right, with the columns the run covers.
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

/// The column `glyph` sits in, counted in cells and never in the bytes of a glyph wider than one:
/// a meter's own cells are three bytes each, and a slash past them is still one column along.
pub(crate) fn column_of(row: &str, glyph: char) -> Option<usize> {
    row.chars().position(|drawn| drawn == glyph)
}
