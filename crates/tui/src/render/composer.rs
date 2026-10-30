//! The composer: what is being typed, in a box, with the cursor where it
//! belongs.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

use crate::app::App;
use crate::theme;

pub fn draw(app: &App, area: Rect, buf: &mut Buffer) -> Position {
    // The composer is the one thing on screen that is always the user's, so
    // it floats on its own panel: the border is the prompt's colour while it
    // holds the focus, and steps back to faint while a run has it (ADR-0016).
    let border = if app.is_running() {
        theme::faint()
    } else {
        theme::prompt()
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .style(theme::surface())
        .border_style(border)
        .title(ratatui::text::Span::styled(" prompt ", border));
    let inner = block.inner(area);
    let lines: Vec<Line<'static>> = app
        .composer()
        .rows()
        .iter()
        .map(|row| Line::from(row.clone()))
        .collect();
    Paragraph::new(lines)
        .style(theme::surface())
        .render(inner, buf);
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
