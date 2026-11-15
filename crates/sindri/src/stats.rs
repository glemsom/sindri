//! The ledger's second adapter: the screen's own layout, its height, and its rows drawn on the card
//! that layout makes as tall as itself.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ledger::{self, Facts, Row, Shape};
use crate::palette::Palette;

/// The rows the screen holds, so the card it is drawn on is this many rows plus its two borders.
pub fn height(facts: &Facts, width: u16) -> u16 {
    u16::try_from(sheet(facts, width).len()).unwrap_or(u16::MAX)
}

/// Draws the sheet in `body`, which is the band the card gives it: the card's own row and column
/// are where the sheet starts and its height is where it stops.
pub fn paint(facts: &Facts, body: Rect, buffer: &mut Buffer, palette: &Palette) {
    ledger::draw(&sheet(facts, body.width), body, buffer, palette);
}

/// The screen's whole layout at `width`: every block it can draw, with no band to cap it.
fn sheet(facts: &Facts, width: u16) -> Vec<Row> {
    ledger::rows(facts, Shape::Screen, Rect::new(0, 0, width, u16::MAX), None)
}
