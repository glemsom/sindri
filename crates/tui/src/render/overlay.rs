//! What covers the transcript when something has to: the list of every binding
//! `?` opens, and a command's untruncated output (ADR-0011 §9, §14). Both are
//! boxes in the middle of the frame, closed with `Esc`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget, Wrap};

use crate::app::App;
use crate::overlay::{Overlay, Spill};
use crate::theme;

/// How wide the keys column of the help overlay is.
const KEYS: usize = 16;

/// Draws the overlay that has the screen, over everything else.
pub fn draw(app: &App, area: Rect, buf: &mut Buffer) {
    let Some(overlay) = app.overlay() else {
        return;
    };
    let (title, rows) = match overlay {
        Overlay::Help => (" keys ", marked(&Overlay::Help.rows())),
        Overlay::Spill(spill) => (" output ", scrolled(spill, area)),
    };
    let height = (rows.len() as u16).saturating_add(2).min(area.height);
    let width = area.width.saturating_sub(8).clamp(20, 110);
    let boxed = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    Clear.render(boxed, buf);
    let block = Block::bordered()
        .border_style(theme::faint())
        .title(Span::styled(title, theme::faint()));
    let inner = block.inner(boxed);
    let lines: Vec<Line<'static>> = rows
        .into_iter()
        .map(|(keys, what)| {
            let keys = format!("{keys:<KEYS$}");
            Line::from(vec![
                Span::styled(keys, theme::prompt()),
                Span::styled(what, theme::faint()),
            ])
        })
        .collect();
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(inner, buf);
    block.render(boxed, buf);
}

/// The help's rows, with the mouse gesture beside the key that does the same.
fn marked(rows: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
    rows.iter()
        .map(|(keys, what)| (*keys, what.clone()))
        .collect()
}

/// A spill file's rows, from where the reader has scrolled, as many as fit.
fn scrolled(spill: &Spill, area: Rect) -> Vec<(&'static str, String)> {
    let mut rows = spill.shown();
    let skip = spill.scroll.min(rows.len().saturating_sub(1));
    let mut rows: Vec<(&'static str, String)> = rows.drain(skip..).collect();
    rows.truncate(area.height.saturating_sub(4) as usize);
    rows
}
