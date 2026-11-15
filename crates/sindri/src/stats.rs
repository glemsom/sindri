//! The ledger's second adapter: the screen's own layout, its height, and its rows drawn on the card
//! that layout makes as tall as itself.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::ledger::{self, Facts, Row, Shape};
use crate::palette::Palette;

/// The rows the screen holds, so the card it is drawn on is this many rows plus its two borders.
pub fn height(facts: &Facts, width: u16) -> u16 {
    u16::try_from(laid(facts, width).len()).unwrap_or(u16::MAX)
}

/// Draws the layout in `body`, the band the card gives it: the rows the card holds are the cap, so
/// one past them is not drawn.
pub fn paint(facts: &Facts, body: Rect, buffer: &mut Buffer, palette: &Palette) {
    ledger::draw(&laid(facts, body.width), body, buffer, palette);
}

/// The screen's whole layout at `width`: every block it can draw, with no band to cap it.
fn laid(facts: &Facts, width: u16) -> Vec<Row> {
    ledger::rows(facts, Shape::Screen, Rect::new(0, 0, width, u16::MAX), None)
}
