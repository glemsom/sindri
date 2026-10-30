//! The completion, as a column just above the composer: the eight nearest
//! matches — `@` paths or `/` commands, whichever was asked for — the chosen
//! one marked, and a word when a walk stopped at its cap.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::markdown::text;
use crate::theme;

pub fn draw(app: &App, area: Rect, buf: &mut Buffer) {
    let Some(completion) = app.completion() else {
        return;
    };
    if completion.items.is_empty() || area.width == 0 {
        return;
    }
    let width = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    if completion.capped {
        lines.push(Line::from(Span::styled(
            text::clip(
                "· the walk stopped at its cap: not every path is here",
                width.saturating_sub(2),
            ),
            theme::bad(),
        )));
    }
    let room = (area.height as usize).saturating_sub(lines.len());
    for (index, path) in completion.items.iter().enumerate().take(room) {
        let (marker, style) = if index == completion.chosen {
            ("▸ ", theme::prompt())
        } else {
            ("  ", theme::faint())
        };
        let shown = text::clip(path, width.saturating_sub(2));
        lines.push(Line::from(vec![
            Span::styled(marker, style),
            Span::styled(shown, style),
        ]));
    }
    Paragraph::new(lines).render(area, buf);
}
