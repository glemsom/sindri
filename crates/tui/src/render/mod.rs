//! The narrowest terminal that can hold the rail as a column; below this it
//! is a strip.

mod call;
mod card;
mod completion;
pub mod composer;
pub mod hello;
pub(crate) mod layout;
pub(crate) mod mark;
mod overlay;
pub(crate) mod pictures;
mod queue;
mod rail;
pub(crate) mod rows;
pub(crate) mod seam;
mod settings;
pub mod transcript;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::app::App;
use crate::theme;

pub use crate::card::Card;

pub const RAIL_COLUMNS: u16 = 32;
pub const RAIL_MIN_WIDTH: u16 = 100;
/// The rail's left inset: the cells between the seam and the label's field. One
/// clears the seam's light strip, which is drawn in the gap beside the rail's
/// first column; one more holds the first word of a fact off it, so no word sits
/// against the line — whether the terminal draws the strip there or plain canvas.
pub const RAIL_INSET: usize = 2;
/// The canvas between the words and the rail. Two cells, not one: a picture's
/// escape is wider than the cell that carries it, so `Buffer::diff` leaves the
/// cell after it unpainted, and the strip the margin draws is a picture. The
/// strip takes both cells, so the one the diff costs it is its own and never
/// the rail's first column or the face drawn above it.
pub const GAP_COLUMNS: u16 = 2;
/// The footer's hints, in the one order they are worth saying: the keys that
/// send, open the list and leave first, because a terminal too narrow for all
/// of them is exactly where they are least discoverable.
const HINTS: [&str; 7] = [
    "Enter send",
    "? keys",
    "Ctrl-Q quit",
    "@ paths",
    "/ commands",
    "Shift+Enter newline",
    "PgUp/PgDn scroll",
];

/// What separates one hint from the next.
const HINT_GAP: &str = " · ";

pub fn draw(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    // Before the layout, so a region the frame draws nothing into is still the
    // theme's.
    canvas(area, frame.buffer_mut());
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
    let (transcript, margin, rail_column, rail_strip) = if area.width >= RAIL_MIN_WIDTH {
        // The terminal's own canvas between the words and the rail: the rail's
        // seam is an edge, and an edge needs air on both sides of it.
        // Where the terminal speaks Kitty graphics the strip of
        // `seam.rs` fills the gap, from the top of the body down to the
        // composer's last row.
        let [left, margin, right] = Layout::horizontal([
            Constraint::Min(1),
            Constraint::Length(GAP_COLUMNS),
            Constraint::Length(RAIL_COLUMNS),
        ])
        .areas(body);
        (left, Some(margin), Some(right), None)
    } else {
        let [above, below] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(body);
        (above, None, None, Some(below))
    };
    // The composer is laid out in the words' column, not across the terminal:
    // its right border stops one column short of the margin the seam's strip
    // runs in, so the light the border walks toward is beside the corner that
    // walk brightens. Every cell to the right of it stays canvas — the rail's
    // column at the composer's rows is the canvas the rail's column below its
    // last fact already is — and the footer, a strip of hints rather than a
    // panel, keeps the whole width.
    let panel = Rect {
        width: transcript.width,
        ..input
    };
    if let Some(completions) = completions {
        completion::draw(app, completions, frame.buffer_mut());
    }

    transcript::draw(app, transcript, frame.buffer_mut());
    // The glow is drawn beside the rail and never over it: the margin has its
    // own rect, and `set_rail_view` is given the rail's. It is carried down to
    // the composer's last row, so the panel is lit for its whole height and the
    // light leaves at the footer under it.
    if let Some(margin) = margin {
        let strip = seam::reach(margin, panel);
        seam::draw(app, strip, frame.buffer_mut());
    }
    match (rail_column, rail_strip) {
        (Some(area), _) => rail::column(app, area, frame.buffer_mut()),
        (None, Some(area)) => rail::strip(app, area, frame.buffer_mut()),
        (None, None) => {}
    }
    if queued > 0 {
        queue::draw(app, regions[1], frame.buffer_mut());
    }
    let cursor = composer::draw(app, panel, frame.buffer_mut());
    frame.set_cursor_position(cursor);
    footer_rows(app, footer, frame.buffer_mut());
    overlay::draw(app, area, frame.buffer_mut());
}

pub fn draw_card(card: &Card, view: &mut crate::card::View, frame: &mut Frame) {
    let area = frame.area();
    // The card does not go through `draw`, so it fills its own canvas: what
    // the last frame left under it is not the surface this one is written on.
    canvas(area, frame.buffer_mut());
    card::draw(card, view, area, frame.buffer_mut());
}

/// Fills a region with the canvas, the surface every word is written on. A
/// resize, a scrolled region, a card or the gap a picture's escape leaves
/// unpainted is the theme's surface then, never the terminal's — a light
/// terminal or one with a background image would otherwise change the frame as
/// surely as a palette would.
fn canvas(area: Rect, buf: &mut Buffer) {
    buf.set_style(area, theme::canvas());
}

fn footer_rows(app: &App, area: Rect, buf: &mut Buffer) {
    let mut prefix = String::new();
    // A cancel is answered where the "working" was: the request in flight is
    // not interrupted, so the frame says what it is waiting for instead of
    // going quiet until it lands. The queue states its own size.
    if app.is_cancelling() {
        prefix.push_str("cancelling… · ");
    } else if app.is_running() {
        prefix.push_str("working · ");
    }
    // Rows that arrived below a reader who is not on the tail: the frame says
    // so, and stops saying it the moment they come back to it. It
    // is drawn against the right edge, so the hints are given only what is
    // left of the row: a hint is never the thing that gets cut.
    let new = app.window.new_rows();
    let rows = if new == 1 { "row" } else { "rows" };
    let badge = (new > 0).then(|| format!("↓ {new} new {rows}"));
    let badge = badge.filter(|badge| badge.chars().count() < area.width as usize);
    let room = (area.width as usize)
        .saturating_sub(prefix.chars().count())
        .saturating_sub(badge.as_ref().map_or(0, |badge| badge.chars().count() + 1));
    let line = format!("{prefix}{}", hints(area.width as usize, room));
    Paragraph::new(Line::from(Span::styled(line, theme::faint()))).render(area, buf);
    if let Some(badge) = badge {
        let columns = badge.chars().count() as u16;
        // Only the badge's own cells: a hint it sits beside is never drawn
        // under it, because it was given the room first.
        let at = Rect {
            x: area.x + area.width - columns,
            width: columns,
            ..area
        };
        Paragraph::new(Span::styled(badge, theme::prompt())).render(at, buf);
    }
}

/// The hint line for a terminal `width` columns wide with `room` cells to
/// spare: all seven hints from 120 columns up, five from 100, three from 60
/// and never fewer than the two that send and open the list. A hint is a
/// whole thing or it is not there at all — the set shrinks rather than the
/// word being cut.
fn hints(width: usize, room: usize) -> String {
    let mut count = match width {
        w if w >= 120 => HINTS.len(),
        w if w >= 100 => 5,
        w if w >= 60 => 3,
        _ => 2,
    };
    loop {
        let line = HINTS[..count].join(HINT_GAP);
        if line.chars().count() <= room || count == 1 {
            return line;
        }
        count -= 1;
    }
}
