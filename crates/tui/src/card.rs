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
/// against the same numbers, so the card stops where its last screenful is
/// full (the rule for anything that scrolls).
#[derive(Debug, Clone, Default)]
pub struct View {
    scroll: usize,
    total: usize,
    visible: usize,
}

impl View {
    /// The rows the card drew, so a card short enough to fit does not scroll at
    /// all and a paged one keeps its place across a redraw.
    pub fn settled(&mut self, total: usize, visible: usize) {
        self.total = total;
        self.visible = visible.max(1);
        self.scroll = self.scroll.min(self.last());
    }

    fn last(&self) -> usize {
        self.total.saturating_sub(self.visible)
    }

    /// The rows the card shows, from where the reader is.
    pub fn window(&self) -> std::ops::Range<usize> {
        let start = self.scroll.min(self.last());
        start..(start + self.visible).min(self.total)
    }

    /// `PgUp`/`PgDn`: one screenful of the card at a time, from the first row
    /// to the last and no further.
    pub fn page(&mut self, up: bool) {
        self.scroll = if up {
            self.scroll.saturating_sub(self.visible)
        } else {
            self.scroll.saturating_add(self.visible).min(self.last())
        };
    }

    /// Whether the card runs past the screen above or below: what the box marks
    /// so a reader knows there is more without scrolling for it.
    pub fn above(&self) -> bool {
        self.window().start > 0
    }

    pub fn below(&self) -> bool {
        self.window().end < self.total
    }

    /// Whether there is anything to page at all. A card with nothing beyond its
    /// bottom is left by every key, including the page keys.
    pub fn more(&self) -> bool {
        self.total > self.visible
    }
}

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
