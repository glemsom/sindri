//! The composer: what is being typed, in a box, with the cursor where it
//! belongs, and a border lit from the rail's side.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

use crate::app::App;
use crate::theme;

/// The border's walk: the top and bottom rules are lit from the rail's side, so
/// the panel's right-hand end is the seam's ink and its left-hand end is the ink
/// it has when it is quiet, or when it holds the keyboard. Only the rules are
/// walked, and the corners take the walk's own ends, so the silhouette is
/// brightest where the light lands and the panel reads as part of the rail's
/// column rather than a box beside it.
///
/// The ` prompt ` title and the two vertical rules keep one ink: a walk across
/// one cell says nothing, and the title's letters are a word, not a rule.
fn lit(area: Rect, buf: &mut Buffer, ink: Color) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // The walk is a function of the column, so its step is the panel's width:
    // the last column is the far end, and a panel one column wide is all of it.
    let span = f32::from(area.width.saturating_sub(1)).max(1.0);
    for y in [area.y, area.y + area.height.saturating_sub(1)] {
        for x in area.x..area.x + area.width {
            let cell = &mut buf[(x, y)];
            if !matches!(cell.symbol(), "─" | "╭" | "╮" | "╰" | "╯") {
                continue;
            }
            let t = f32::from(x - area.x) / span;
            cell.set_fg(theme::ramp(ink, theme::SEAM, t));
        }
    }
}

/// What the composer says while it holds nothing: the three ways a prompt can
/// begin — asked, a path, or a command — so the first thing typed is not a
/// guess (issue #46). It is drawn only over a composer with no text in it at
/// all, so it never sits in front of a draft.
pub const PLACEHOLDER: &str = "Ask, or drop a log here · @ path · / command";

pub fn draw(app: &App, area: Rect, buf: &mut Buffer) -> Position {
    // The composer is the one thing on screen that is always the user's, so
    // it floats on its own panel: the border is the prompt's colour while it
    // holds the focus, and steps back to faint while a run has it.
    let (border, ink) = if app.is_running() {
        (theme::faint(), theme::FAINT)
    } else {
        (theme::prompt(), theme::PROMPT)
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .style(theme::surface())
        .border_style(border)
        .title(ratatui::text::Span::styled(" prompt ", border));
    let inner = block.inner(area);
    let lines: Vec<Line<'static>> = match app.missing {
        // What this composer cannot do, where the prompt would be: a run with
        // no key cannot send one, and saying so is the frame's.
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
    lit(area, buf, ink);
    let (column, row) = app.composer().cursor();
    Position::new(
        inner.x.saturating_add(column),
        inner
            .y
            .saturating_add(row)
            .min(inner.y + inner.height.saturating_sub(1)),
    )
}
