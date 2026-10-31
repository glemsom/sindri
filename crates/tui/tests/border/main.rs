//! The composer's border as a picture, held to what it looks like rather than
//! to a screenshot: a pure function of a rect in cells, a cell's pixels, the
//! ink it is drawn in and the cells its label takes. Four sections — the
//! shape it has, the weight it spends, the ink it is drawn in, and the corner
//! it cuts — and the helpers they share.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod corner;
mod ink;
mod shape;
mod weight;

use ratatui::layout::Rect;
use ratatui::style::Color;
use sindri_tui::{border, theme};

use shape::*;

#[test]
fn the_picture_is_the_rect_it_is_asked_for_to_the_pixel() {
    let frame = frame();
    assert_eq!(
        frame.dimensions(),
        (
            u32::from(PANEL.width) * u32::from(CELL.0),
            u32::from(PANEL.height) * u32::from(CELL.1)
        ),
        "the frame is the panel's cells, each one the terminal's cell in pixels"
    );
}
