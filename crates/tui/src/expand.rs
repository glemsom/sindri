//! Opening a call, and the reasoning beside it, in place.
//! The frame has no cursor of its own: `Enter` on an empty composer means the
//! entry the reader last touched — or, if they have touched none, the newest
//! one they can open — and a click means the entry whose row it landed on.
//! Both end in the same toggle, so no gesture is the only way in.

use crate::app::App;
use crate::entry::Entry;

impl App {
    /// Opens the chosen entry, or the newest one there is anything to open
    /// in, and says whether it found one.
    pub(crate) fn reveal(&mut self) -> bool {
        let chosen = self
            .chosen
            .filter(|index| self.entries.get(*index).is_some_and(Entry::expandable))
            .or_else(|| self.entries.iter().rposition(Entry::expandable));
        match chosen {
            Some(index) => {
                self.chosen = Some(index);
                self.toggle(index);
                true
            }
            None => false,
        }
    }

    /// A click on an entry's own row: that entry, open or folded, as the row
    /// says.
    pub(crate) fn toggle(&mut self, index: usize) {
        if self.entries.get_mut(index).is_some_and(Entry::toggle) {
            self.dirty();
            // An entry that opens or folds moves every row below it, and not
            // at the tail: what the window counted there is stale.
            self.window.forget();
        }
    }
}
