//! The ledger's second adapter: the band a screen is handed, and the rows the ledger lays in it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ledger::{self, Facts, Shape};
use crate::palette::Palette;

pub fn paint(facts: &Facts, area: Rect, buffer: &mut Buffer, palette: &Palette) {
    ledger::draw(&ledger::rows(facts, Shape::Screen, area), buffer, palette);
}
