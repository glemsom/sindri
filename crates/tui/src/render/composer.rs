//! The composer: what is being typed, in a box, with the cursor where it
//! belongs.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::app::App;
use crate::theme;

/// Draws the composer and returns where the cursor goes.
pub fn draw(app: &App, area: Rect, buf: &mut Buffer) -> Position {
    let block = Block::bordered()
        .border_style(theme::faint())
        .title(ratatui::text::Span::styled(" prompt ", theme::faint()));
    let inner = block.inner(area);
    let lines: Vec<Line<'static>> = app
        .composer()
        .rows()
        .iter()
        .map(|row| Line::from(row.clone()))
        .collect();
    Paragraph::new(lines).render(inner, buf);
    block.render(area, buf);
    let (column, row) = app.composer().cursor();
    Position::new(
        inner.x.saturating_add(column),
        inner
            .y
            .saturating_add(row)
            .min(inner.y + inner.height.saturating_sub(1)),
    )
}
