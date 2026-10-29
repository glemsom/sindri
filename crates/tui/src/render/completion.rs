//! The `@` completion, as a column of paths just above the composer: the eight
//! nearest matches, the chosen one marked (ADR-0011 §16).

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::theme;

/// Draws the offers that are open, chosen row marked.
pub fn draw(app: &App, area: Rect, buf: &mut Buffer) {
    let Some(completion) = app.completion() else {
        return;
    };
    if completion.items.is_empty() || area.width == 0 {
        return;
    }
    let width = area.width as usize;
    let lines: Vec<Line<'static>> = completion
        .items
        .iter()
        .enumerate()
        .take(area.height as usize)
        .map(|(index, path)| {
            let (marker, style) = if index == completion.chosen {
                ("▸ ", theme::prompt())
            } else {
                ("  ", theme::faint())
            };
            let shown: String = path.chars().take(width.saturating_sub(2)).collect();
            Line::from(vec![
                Span::styled(marker, style),
                Span::styled(shown, style),
            ])
        })
        .collect();
    Paragraph::new(lines).render(area, buf);
}
