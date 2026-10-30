//! The composer: what is being typed, in a box, with the cursor where it
//! belongs.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

use crate::app::App;
use crate::theme;

/// What the composer says while it holds nothing: the three ways a prompt can
/// begin — asked, a path, or a command — so the first thing typed is not a
/// guess (issue #46). It is drawn only over a composer with no text in it at
/// all, so it never sits in front of a draft.
pub const PLACEHOLDER: &str = "Ask, or drop a log here · @ path · / command";

pub fn draw(app: &App, area: Rect, buf: &mut Buffer) -> Position {
    // The composer is the one thing on screen that is always the user's, so
    // it floats on its own panel: the border is the prompt's colour while it
    // holds the focus, and steps back to faint while a run has it (ADR-0011 §23).
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
    let lines: Vec<Line<'static>> = match app.missing {
        // What this composer cannot do, where the prompt would be: a run with
        // no key cannot send one, and saying so is the frame's (ADR-0004).
        Some(missing) if app.composer().is_blank() => {
            vec![Line::from(ratatui::text::Span::styled(
                missing.composer(),
                theme::faint(),
            ))]
        }
        // An empty composer invites the first keystroke; a blank one is not
        // the same thing — a space is a draft, and a draft is never shadowed.
        None if app.composer().text().is_empty() => vec![Line::from(ratatui::text::Span::styled(
            PLACEHOLDER,
            theme::faint(),
        ))],
        _ => app
            .composer()
            .rows()
            .iter()
            .map(|row| Line::from(row.clone()))
            .collect(),
    };
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
