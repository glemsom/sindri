//! The frame's drawing: the transcript, the rail, the composer and every other
//! surface, as rows on the theme's canvas, each part in a module of its own
//! beside this one.
//!
//! `RAIL_MIN_WIDTH` is the narrowest terminal that can hold the rail as a
//! column; below it the rail is a strip, and the drawing here is what reads it.

mod call;
mod card;
mod completion;
pub mod composer;
pub mod hello;
pub(crate) mod layout;
pub(crate) mod mark;
mod model;
mod overlay;
pub(crate) mod pictures;
mod queue;
mod rail;
pub(crate) mod rows;
pub(crate) mod seam;
mod settings;
pub(crate) mod step;
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

/// What separates one word of the footer from the next: the run's word from
/// the first hint, and each hint from the one after it.
const WORD_GAP: &str = " · ";

pub fn draw(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    // Before the layout, so a region the frame draws nothing into is still the
    // theme's.
    canvas(area, frame.buffer_mut());
    // The words' column is known before the vertical layout because the queue's
    // own height is measured at the width its panel is drawn at: narrowing the
    // rect alone would leave the panel a row short, its text wrapped at the
    // terminal's width and its last rows dropped without the count that stands
    // in for them.
    let words = words(area.width);
    // The queue is a row the layout gives up only when something is waiting
    // in it: an empty panel would be a row stolen from the transcript. It
    // never takes the room the composer and the footer need, because a queue
    // drawn over what is being typed answers the wrong question.
    let queued =
        queue::height(app, words).min(area.height.saturating_sub(app.composer().height() + 2));
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
            Constraint::Length(words),
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
        width: words,
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
        // The queue takes the row above the composer, and only as wide as the
        // words: the two panels end on one column, so the seam's strip runs from
        // the top of the body to the composer's last row unbroken through it.
        let band = Rect {
            width: words,
            ..regions[1]
        };
        queue::draw(app, band, frame.buffer_mut());
    }
    let cursor = composer::draw(app, panel, frame.buffer_mut());
    frame.set_cursor_position(cursor);
    footer_rows(app, footer, frame.buffer_mut());
    // An overlay that a reader types into says where the terminal's cursor
    // belongs, and it is the overlay that has the screen: the modal's row is a
    // field, so the cursor is drawn in it rather than left behind the box on a
    // composer the reader cannot see.
    if let Some(at) = overlay::draw(app, area, frame.buffer_mut()) {
        frame.set_cursor_position(at);
    }
}

/// Where the words end: the columns left of the canvas between the words and the
/// rail, and the one width every panel in that column is measured and drawn at,
/// so the queue and the composer cannot disagree about where the frame ends.
/// Below `RAIL_MIN_WIDTH` the rail is a strip inside the body and the words have
/// the whole of it, which is what those panels drew at there.
fn words(width: u16) -> u16 {
    if width >= RAIL_MIN_WIDTH {
        width.saturating_sub(GAP_COLUMNS + RAIL_COLUMNS)
    } else {
        width
    }
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
    // Rows that arrived below a reader who is not on the tail: the frame says
    // so, and stops saying it the moment they come back to it. The badge is
    // drawn against the right edge, and its cells are given to it first — so
    // the words are given only what is left of the row, and a hint is never
    // the thing that gets cut. Only a terminal too narrow to hold the badge at
    // all goes without one.
    let new = app.window.new_rows();
    let rows = if new == 1 { "row" } else { "rows" };
    let badge = (new > 0).then(|| format!("↓ {new} new {rows}"));
    let badge = badge.filter(|badge| badge.chars().count() <= area.width as usize);
    // The cells left of the badge, less the one that keeps a word off it.
    let room = badge.as_ref().map_or(area.width as usize, |badge| {
        (area.width as usize).saturating_sub(badge.chars().count() + 1)
    });
    let line = footer_words(app, area.width as usize, room);
    Paragraph::new(Line::from(Span::styled(line, theme::faint()))).render(area, buf);
    if let Some(badge) = badge {
        let columns = badge.chars().count() as u16;
        // Only the badge's own cells: the words end before them, because the
        // badge was given its cells first.
        let at = Rect {
            x: area.x + area.width - columns,
            width: columns,
            ..area
        };
        Paragraph::new(Span::styled(badge, theme::prompt())).render(at, buf);
    }
}

/// The footer's words left of the badge: the run's own word while it works or
/// waits on a cancel, then the hints, in the one order they are worth saying —
/// as many whole ones as `room` cells hold with the gap between them. A word is
/// a whole thing or it is not drawn, so what a row too narrow for even the
/// first says is nothing at all.
fn footer_words(app: &App, width: usize, room: usize) -> String {
    // A cancel is answered where the "working" was: the request in flight is
    // not interrupted, so the frame says what it is waiting for instead of
    // going quiet until it lands. The queue states its own size.
    let run = if app.is_cancelling() {
        Some("cancelling…")
    } else if app.is_running() {
        Some("working")
    } else {
        None
    };
    let gap = WORD_GAP.chars().count();
    let mut line = String::new();
    let mut left = room;
    for word in run.into_iter().chain(hints_at(width).iter().copied()) {
        // A word spends its own cells and, once the line holds one, the gap
        // before it. Both are counted in cells the terminal draws.
        let need = word.chars().count() + if line.is_empty() { 0 } else { gap };
        if need > left {
            break;
        }
        if !line.is_empty() {
            line.push_str(WORD_GAP);
        }
        line.push_str(word);
        left -= need;
    }
    line
}

/// The hints a terminal `width` columns wide is worth stating, before the
/// badge and a narrow row shorten the set: all seven from 120 columns up, five
/// from 100 and three from 60.
fn hints_at(width: usize) -> &'static [&'static str] {
    let count = match width {
        w if w >= 120 => HINTS.len(),
        w if w >= 100 => 5,
        w if w >= 60 => 3,
        _ => 2,
    };
    &HINTS[..count]
}
