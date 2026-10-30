//! The card: what a broken boot draws instead of a half-drawn
//! interface (ADR-0011).

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

use crate::theme;

#[derive(Debug, Clone)]
pub struct Card {
    pub title: String,
    pub check: String,
    pub detail: String,
}

pub fn draw(card: &Card, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .border_style(theme::bad())
        .title(Span::styled(" sindri ", theme::bad()));
    let inner = block.inner(area);
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(6),
        Constraint::Fill(1),
    ])
    .split(inner);
    let lines = vec![
        Line::from(Span::styled(card.title.clone(), theme::bad())),
        Line::default(),
        Line::from(vec![
            Span::styled("check: ", theme::faint()),
            Span::styled(card.check.clone(), ratatui::style::Style::default()),
        ]),
        Line::default(),
        Line::from(card.detail.clone()),
    ];
    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    paragraph.render(rows[1], buf);
    block.render(area, buf);
}
