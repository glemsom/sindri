//! What covers the transcript when something has to: the list of every binding
//! `?` opens, and a command's untruncated output (ADR-0011 §9, §14). Both are
//! boxes in the middle of the frame, closed with `Esc`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::app::App;
use crate::markdown::text;
use crate::overlay::Overlay;
use crate::theme;

/// How wide the keys column of the help overlay is.
const KEYS: usize = 20;

/// Draws the overlay that has the screen, over everything else.
pub fn draw(app: &mut App, area: Rect, buf: &mut Buffer) {
    let Some(overlay) = app.overlay().cloned() else {
        return;
    };
    let height = match overlay {
        Overlay::Help => (overlay.rows().len() as u16 + 2).min(area.height),
        Overlay::Spill(_) => area.height,
    };
    let width = area.width.saturating_sub(8).clamp(20, 110);
    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    Clear.render(boxed, buf);
    let block = Block::bordered().border_style(theme::faint());
    let inner = block.inner(boxed);
    let (title, lines, held) = match &overlay {
        Overlay::Help => (
            "keys".to_string(),
            overlay
                .rows()
                .into_iter()
                .map(|(keys, what)| help_row(keys, &what))
                .collect::<Vec<_>>(),
            None,
        ),
        Overlay::Spill(spill) => {
            let mut spill = spill.clone();
            spill.settled(inner.height as usize);
            let lines = spill
                .window()
                .iter()
                .map(|line| Line::from(Span::styled(line.clone(), theme::faint())))
                .collect::<Vec<_>>();
            let title = format!("output: {}", spill.path.display());
            (title, lines, Some(spill))
        }
    };
    let block = block.title(Span::styled(format!(" {title} "), theme::faint()));
    Paragraph::new(lines).render(inner, buf);
    block.render(boxed, buf);
    if let Some(spill) = held {
        app.hold_spill(spill);
    }
}

/// One binding: the keys, and what it does with the gesture that also does it.
fn help_row(keys: &str, what: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{keys:<KEYS$}"), theme::prompt()),
        Span::styled(text::clip(what, 200), theme::faint()),
    ])
}
