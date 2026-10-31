//! The wash a drag leaves on the transcript: the cells the selection's one
//! span computation names, tinted with the theme's wash and nothing else. Only
//! the surface is painted, so the ink under it — syntax, a heading, a panel's
//! border — still reads as itself while the row is marked.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::app::App;
use crate::mark::span;
use crate::theme;

pub fn draw(app: &App, buf: &mut Buffer) {
    // An overlay dims the frame and draws over it, so a wash under one is a lit
    // cell nobody can see.
    if app.overlay().is_some() {
        return;
    }
    let area = app.viewport().area;
    for covered in span(app.viewport(), &app.selection()) {
        if covered.cells.is_empty() {
            continue;
        }
        let at = Rect::new(
            area.x + covered.cells.start as u16,
            area.y + covered.row as u16,
            covered.cells.len() as u16,
            1,
        )
        .intersection(area);
        buf.set_style(at, theme::mark());
    }
}
