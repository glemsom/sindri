//! The card: what a broken boot has to say, and what an event does to it.
//! The doctor's words are the content, so the card is measured
//! from them and paged when the screen is shorter than they are; only a key
//! leaves it, never a resize. The drawing is `render::card`.

use crossterm::event::{Event, KeyCode, KeyEventKind};

/// What a failed check says: which failure this was, which check named it, and
/// the doctor's own report about the sandbox it could not build.
#[derive(Debug, Clone)]
pub struct Card {
    pub title: String,
    pub check: String,
    pub detail: String,
}

/// Where the reader is in a card taller than the screen. The renderer measures
/// the rows the words wrapped into and the rows it has for them; the keys clamp
/// against the same numbers. The clamping is [`Scroll`]'s — the help list pages
/// by the rows it was given, the card by the rows it shows — so a card and a
/// list scroll by one rule.
pub use crate::scroll::Scroll as View;

/// What the event that arrived asks of the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// A page key on a card with more rows than the screen: move, do not leave.
    Page { up: bool },
    /// Any other key: the report has been read and the launch ends.
    Leave,
    /// The card is laid out again and nothing is dismissed. A resize arrives
    /// when a `tmux` client attaches or the window is dragged, and neither is
    /// the reader saying they have read the failure.
    Redraw,
}

/// What one event does to a card showing `more` rows than the screen holds.
pub fn answer(event: &Event, more: bool) -> Answer {
    let Event::Key(key) = event else {
        return Answer::Redraw;
    };
    if key.kind != KeyEventKind::Press {
        return Answer::Redraw;
    }
    match (key.code, more) {
        (KeyCode::PageUp, true) => Answer::Page { up: true },
        (KeyCode::PageDown, true) => Answer::Page { up: false },
        _ => Answer::Leave,
    }
}
