//! The queue: what was typed while a run had the composer, drawn whole on a
//! panel of its own above it, with the count the footer could not fit.
//! `↑` takes the newest one back, `Ctrl-X` drops it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

use crate::app::App;
use crate::markdown::text;
use crate::theme;

/// The rows the panel gives the queue's own text. Past them the title states
/// how many are waiting, because the count is the thing that cannot be shown.
const MAX_ROWS: usize = 6;
/// What a queued row is marked with: a quote mark, not a key, because the
/// keys are the frame's.
const MARKER: &str = "› ";

/// How tall the panel is at this width, border and all. `0` when nothing is
/// queued: a panel for nothing is a row taken off the transcript.
pub fn height(app: &App, width: u16) -> u16 {
    match app.queued() {
        0 => 0,
        _ => rows(app, width).len().min(MAX_ROWS) as u16 + 2,
    }
}

/// The title states the size, and only says a count when there is one worth
/// saying: a queue of one is not "1 queued", it is the prompt itself.
fn title(app: &App) -> String {
    match app.queued() {
        0 => String::new(),
        1 => " queued ".to_string(),
        held => format!(" {held} queued "),
    }
}

fn rows(app: &App, width: u16) -> Vec<Line<'static>> {
    // The border's two columns and the marker's are not the words': wrap the
    // words where the panel ends, or the last one is cut in half.
    let inner = (width as usize).saturating_sub(MARKER.chars().count() + 2);
    let mut rows = Vec::new();
    for prompt in app.queue() {
        rows.extend(text::wrap(
            text::glyphs(&[Span::styled(prompt.clone(), Style::new())]),
            MARKER,
            theme::faint(),
            inner,
        ));
    }
    rows
}

pub fn draw(app: &App, area: Rect, buf: &mut Buffer) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .style(theme::surface())
        .border_style(theme::faint())
        .title(Span::styled(title(app), theme::faint()));
    let mut rows = rows(app, area.width);
    rows.truncate(MAX_ROWS);
    Paragraph::new(rows)
        .style(theme::surface())
        .render(block.inner(area), buf);
    block.render(area, buf);
}
