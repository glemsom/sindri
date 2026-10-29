//! Where the pieces go: the transcript column, the rail or its strip, the
//! composer and one line of hints (ADR-0011 §4).

mod card;
mod composer;
mod rail;
mod transcript;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::theme;

pub use card::Card;

/// The rail's width when it is a column.
pub const RAIL_COLUMNS: u16 = 32;
/// The narrowest terminal that can hold the rail as a column; below this it is
/// a strip (ADR-0011 §4).
pub const RAIL_MIN_WIDTH: u16 = 100;
/// The one line of hints under the composer.
const HINTS: &str = "Enter send · Shift+Enter newline · Ctrl-C cancel · Ctrl-Q quit";

/// Draws the whole frame.
pub fn draw(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let [body, input, footer] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(app.composer().height()),
        Constraint::Length(1),
    ])
    .areas(area);

    let (transcript, rail_column, rail_strip) = if area.width >= RAIL_MIN_WIDTH {
        let [left, right] =
            Layout::horizontal([Constraint::Min(1), Constraint::Length(RAIL_COLUMNS)]).areas(body);
        (left, Some(right), None)
    } else {
        let [above, below] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(body);
        (above, None, Some(below))
    };

    transcript::draw(app, transcript, frame.buffer_mut());
    match (rail_column, rail_strip) {
        (Some(area), _) => rail::column(app, area, frame.buffer_mut()),
        (None, Some(area)) => rail::strip(app, area, frame.buffer_mut()),
        (None, None) => {}
    }
    let cursor = composer::draw(app, input, frame.buffer_mut());
    frame.set_cursor_position(cursor);
    footer_rows(app, footer, frame.buffer_mut());
}

/// The card a broken boot draws, across the whole screen.
pub fn draw_card(card: &Card, frame: &mut Frame) {
    let area = frame.area();
    card::draw(card, area, frame.buffer_mut());
}

/// The one line under the composer: the keys, and what is waiting.
fn footer_rows(app: &App, area: Rect, buf: &mut Buffer) {
    let mut hints = String::new();
    if app.is_running() {
        hints.push_str("working · ");
    }
    hints.push_str(HINTS);
    if app.queued() > 0 {
        hints.push_str(&format!(" · {} queued", app.queued()));
    }
    Paragraph::new(Line::from(Span::styled(hints, theme::faint()))).render(area, buf);
}
