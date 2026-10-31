//! The record picker on the raised tier: one row per record, newest first, and
//! on each row the four facts a record is chosen by — when it ran, what model,
//! how far it got, why it stopped — then the first thing it was asked.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::markdown::text;
use crate::sessions::Sessions;
use crate::theme;

pub fn draw(sessions: &Sessions, area: Rect) -> (u16, String, Vec<Line<'static>>) {
    let width = area.width.max(1) as usize;
    // Nothing is ever pruned, so the list outgrows the box: it is windowed
    // around the row the reader is on, and the window moves with them.
    let height = area.height.saturating_sub(2).max(1) as usize;
    let first = sessions.chosen().saturating_add(1).saturating_sub(height);
    let shown = &sessions.rows()[first..sessions.rows().len().min(first + height)];
    let mut lines = Vec::with_capacity(shown.len());
    for (offset, row) in shown.iter().enumerate() {
        let index = first + offset;
        let chosen = index == sessions.chosen();
        let turns = match row.turns {
            Some(1) => "1 turn".to_string(),
            Some(turns) => format!("{turns} turns"),
            // A run that never finished counted no turns: the record has none,
            // and a picker that printed a zero would be inventing one.
            None => "— turns".to_string(),
        };
        let head = format!(
            "{} {}  {} · {} · {}",
            if chosen { "›" } else { " " },
            sindri_core::clock::at(row.started_ms),
            row.model,
            turns,
            row.stopped
        );
        let left = width.saturating_sub(head.chars().count() + 2);
        let prompt = text::clip(row.prompt.as_deref().unwrap_or("(no prompt)"), left);
        let (head_style, prompt_style) = if chosen {
            (theme::prompt(), ratatui::style::Style::new())
        } else {
            (theme::faint(), theme::faint())
        };
        lines.push(Line::from(vec![
            Span::styled(head, head_style),
            Span::styled(format!("  {prompt}"), prompt_style),
        ]));
    }
    let title = format!("sessions · {} records", sessions.rows().len());
    (
        (lines.len() as u16).saturating_add(2).min(area.height),
        title,
        lines,
    )
}
