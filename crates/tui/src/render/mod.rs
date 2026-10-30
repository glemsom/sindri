//! The narrowest terminal that can hold the rail as a column; below this it
//! is a strip (ADR-0011).

mod call;
mod card;
mod completion;
mod composer;
pub(crate) mod layout;
mod overlay;
mod queue;
mod rail;
pub(crate) mod rows;
mod sessions;
mod settings;
pub mod transcript;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::theme;

pub use crate::card::Card;

pub const RAIL_COLUMNS: u16 = 32;
pub const RAIL_MIN_WIDTH: u16 = 100;
const GAP_COLUMNS: u16 = 1;
const HINTS: &str = "Enter send · Shift+Enter newline · PgUp/PgDn scroll · @ paths · / commands · ? keys · Ctrl-Q quit";

pub fn draw(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    // The queue is a row the layout gives up only when something is waiting
    // in it: an empty panel would be a row stolen from the transcript. It
    // never takes the room the composer and the footer need, because a queue
    // drawn over what is being typed answers the wrong question.
    let queued =
        queue::height(app, area.width).min(area.height.saturating_sub(app.composer().height() + 2));
    let mut rows = vec![Constraint::Min(1)];
    if queued > 0 {
        rows.push(Constraint::Length(queued));
    }
    rows.push(Constraint::Length(app.composer().height()));
    rows.push(Constraint::Length(1));
    let regions = Layout::vertical(rows).split(area);
    let body = regions[0];
    let footer = regions[regions.len() - 1];
    let input = regions[regions.len() - 2];

    let (body, completions) = match app.completion() {
        Some(completion) if !completion.items.is_empty() => {
            let rows = (completion.items.len() + usize::from(completion.capped)) as u16;
            let [above, below] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(rows)]).areas(body);
            (above, Some(below))
        }
        _ => (body, None),
    };
    let (transcript, rail_column, rail_strip) = if area.width >= RAIL_MIN_WIDTH {
        // One column of the terminal's own canvas between the words and the
        // rail: the rail is a surface, and a surface needs an edge (ADR-0016).
        let [left, _, right] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(GAP_COLUMNS),
            Constraint::Length(RAIL_COLUMNS),
        ])
        .areas(body);
        (left, Some(right), None)
    } else {
        let [above, below] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(body);
        (above, None, Some(below))
    };
    if let Some(completions) = completions {
        completion::draw(app, completions, frame.buffer_mut());
    }

    transcript::draw(app, transcript, frame.buffer_mut());
    match (rail_column, rail_strip) {
        (Some(area), _) => rail::column(app, area, frame.buffer_mut()),
        (None, Some(area)) => rail::strip(app, area, frame.buffer_mut()),
        (None, None) => {}
    }
    if queued > 0 {
        queue::draw(app, regions[1], frame.buffer_mut());
    }
    let cursor = composer::draw(app, input, frame.buffer_mut());
    frame.set_cursor_position(cursor);
    footer_rows(app, footer, frame.buffer_mut());
    overlay::draw(app, area, frame.buffer_mut());
}

pub fn draw_card(card: &Card, view: &mut crate::card::View, frame: &mut Frame) {
    let area = frame.area();
    card::draw(card, view, area, frame.buffer_mut());
}

fn footer_rows(app: &App, area: Rect, buf: &mut Buffer) {
    let mut hints = String::new();
    // A cancel is answered where the "working" was: the request in flight is
    // not interrupted, so the frame says what it is waiting for instead of
    // going quiet until it lands (ADR-0004). The queue states its own size.
    if app.is_cancelling() {
        hints.push_str("cancelling… · ");
    } else if app.is_running() {
        hints.push_str("working · ");
    }
    hints.push_str(HINTS);
    Paragraph::new(Line::from(Span::styled(hints, theme::faint()))).render(area, buf);
    // Rows that arrived below a reader who is not on the tail: the frame says
    // so, and stops saying it the moment they come back to it (issue #45). It
    // is drawn against the right edge, so a hint it cannot sit beside is the
    // thing that gets cut and never the count.
    let new = app.window.new_rows();
    if new > 0 {
        let rows = if new == 1 { "row" } else { "rows" };
        let badge = format!("↓ {new} new {rows}");
        let columns = badge.width() as u16;
        if columns < area.width {
            // Only the badge's own cells: a hint it sits beside is cut where
            // the badge begins and never cleared across the whole row.
            let at = Rect {
                x: area.x + area.width - columns,
                width: columns,
                ..area
            };
            Paragraph::new(Span::styled(badge, theme::prompt())).render(at, buf);
        }
    }
}
