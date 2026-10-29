//! What covers the transcript when something has to: the list of every binding
//! `?` opens, and a command's untruncated output (ADR-0011 §9, §14). Both are
//! boxes in the middle of the frame, closed with `Esc`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::app::App;
use crate::markdown::text;
use crate::overlay::{Overlay, help_rows};
use crate::theme;

/// How wide the keys column of the help overlay is.
const KEYS: usize = 20;

/// Draws the overlay that has the screen, over everything else. The overlay is
/// borrowed rather than cloned: a spill of a megabyte is a window per frame,
/// never a copy of itself (ADR-0013).
pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let Some(overlay) = app.overlay_mut() else {
        return;
    };
    let height = match &*overlay {
        Overlay::Help => (help_rows().len() as u16 + 2).min(area.height),
        Overlay::Spill(_) => area.height,
    };
    let width = area.width.saturating_sub(8).clamp(20, 110);
    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    let block = Block::bordered().border_style(theme::faint());
    let inner = block.inner(boxed);
    match overlay {
        Overlay::Help => {
            let lines = help_rows()
                .into_iter()
                .map(|(keys, what)| help_row(keys, &what))
                .collect::<Vec<_>>();
            render(buf, boxed, inner, "keys", lines);
        }
        Overlay::Spill(spill) => {
            spill.settled(inner.height as usize);
            let title = format!(
                "output: {} · {} lines",
                spill.path.display(),
                spill.lines.len()
            );
            let lines = spill
                .window()
                .iter()
                .map(|line| Line::from(Span::styled(line.as_str(), theme::faint())))
                .collect::<Vec<_>>();
            render(buf, boxed, inner, &title, lines);
        }
    }
}

/// One box, cleared and drawn over whatever was underneath.
fn render(buf: &mut Buffer, boxed: Rect, inner: Rect, title: &str, lines: Vec<Line<'_>>) {
    Clear.render(boxed, buf);
    Paragraph::new(lines).render(inner, buf);
    Block::bordered()
        .border_style(theme::faint())
        .title(Span::styled(format!(" {title} "), theme::faint()))
        .render(boxed, buf);
}

/// One binding: the keys, and what it does with the gesture that also does it.
fn help_row(keys: &str, what: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{keys:<KEYS$}"), theme::prompt()),
        Span::styled(text::clip(what, 200), theme::faint()),
    ])
}
