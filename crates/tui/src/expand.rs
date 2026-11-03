//! Opening a call in place.
//! The frame has no cursor of its own: `Enter` on an empty composer means the
//! entry the reader last touched — or, if they have touched none, the newest
//! one they can open — and a click means the entry whose row it landed on.
//! Both end in the same toggle, so no gesture is the only way in, and both are
//! reading rather than watching: a toggle on a following window takes the
//! reader's place at that entry, and `End` is the key that gives it back.
//!
//! A thought is not one of them: the whole of it is a row of the thread, so
//! `Enter` and a click have nothing to open in it and no row of it moves for a
//! keystroke.
//!
//! An entry is what this opens, and a diagram is not one: it is a block inside
//! an answer's entry, one of however many that answer holds. Opening it in place
//! would need a click target and a place finer than an entry — a second
//! expansion under the first one's name — so #122's caption is not a target, and
//! what a reader who wants a diagram's source is given is its copy: the block
//! copies as the fence body it was drawn from (`markdown::mermaid`).

use crate::app::App;
use crate::entry::Entry;
use crate::window::Ask;

impl App {
    /// The entry a key that opens something means: the one the reader last
    /// touched, where there is something in it to open, or the newest one there
    /// is.
    fn target(&self, openable: fn(&Entry) -> bool) -> Option<usize> {
        self.chosen
            .filter(|index| self.thread.get(*index).is_some_and(openable))
            .or_else(|| self.thread.position(openable))
    }

    /// Opens the chosen entry, or the newest one there is anything to open
    /// in, and says whether it found one.
    pub(crate) fn reveal(&mut self) -> bool {
        match self.target(Entry::expandable) {
            Some(index) => {
                self.chosen = Some(index);
                self.toggle(index);
                true
            }
            None => false,
        }
    }

    /// A click on an entry's own row: that entry, open or folded, as the row
    /// says. The window is corrected here rather than re-placed by every frame,
    /// because the delta is known at the one gesture that causes it.
    pub(crate) fn toggle(&mut self, index: usize) {
        // An entry with nothing to open is not mutated at all: a toggle that
        // toggled nothing would forget a height that did not change.
        if !self.thread.get(index).is_some_and(Entry::expandable) {
            return;
        }
        self.thread.edit(index, |entry| {
            entry.toggle();
        });
        let (entry, row) = self.window.at;
        let held = !self.window.follow;
        // A reader who opens a call while the column is following is reading
        // rather than watching: the entry becomes the top row of the column and
        // what arrives below it is counted, not followed. `End` takes the tail
        // again, exactly as it does for a reader who scrolled back.
        if !held {
            self.window.want(Ask::Hold(index));
        }
        // An entry that opens or folds moves every row below it, and not at
        // the tail: what the window counted there is stale.
        self.window.forget();
        // A block above the window needs nothing — `at` names a row of a block
        // whose index did not change, which is what the place is an entry and a
        // row for — and one below it moves only rows the window never counted.
        // The block the top row sits in is the case with a row to keep: an
        // expansion inserts its rows below the entry's own row, so an offset
        // past that row no longer means what it did. The place steps back to
        // it, to the row a reader pressed `Enter` on — the only row of a folded
        // call that is there both before and after.
        if held && index == entry {
            self.window.shift(row as isize);
        }
    }
}
