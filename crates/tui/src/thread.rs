//! The transcript as the frame holds it: the entries in order, and the rows each
//! one is worth at the width it was measured at. A mutation is the only way in,
//! and each one forgets exactly the heights it invalidated — so no caller can
//! forget on the thread's behalf and none can forget wrongly.

mod heights;

#[cfg(test)]
mod tests;

use std::time::Duration;

use crate::entry::Entry;
use crate::picture::Store;
use crate::render::rows;

use self::heights::{Heights, Measurement};

/// The entries a frame draws and the heights it has measured them at. The window
/// keeps the reader's place and never learns what a row is worth; the two facts
/// the thread keeps are the ones a row count is made of.
#[derive(Debug, Default)]
pub struct Thread {
    entries: Vec<Entry>,
    heights: Heights,
}

impl Thread {
    /// Appends one entry. A new entry has no height, so nothing above it moved.
    pub fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    /// Changes one entry, forgetting it and every entry after it.
    pub fn edit(&mut self, index: usize, change: impl FnOnce(&mut Entry)) {
        let Some(entry) = self.entries.get_mut(index) else {
            return;
        };
        change(entry);
        self.heights.forget_from(index);
    }

    /// Changes the newest entry, forgetting the newest entry alone: what a
    /// streamed fragment and a line of a call's output know about themselves.
    pub fn edit_last(&mut self, change: impl FnOnce(&mut Entry)) {
        let Some(index) = self.last_index() else {
            return;
        };
        self.edit(index, change);
    }

    /// Keeps the entries `keep` accepts, forgetting every height if any went:
    /// a removal moves every row below it and there is no index to name.
    pub fn keep(&mut self, keep: impl FnMut(&Entry) -> bool) {
        let before = self.entries.len();
        self.entries.retain(keep);
        if self.entries.len() != before {
            self.heights.forget();
        }
    }

    /// Every entry goes, and every height with it: `/new`.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.heights.forget();
    }

    pub fn get(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    pub fn last(&self) -> Option<&Entry> {
        self.entries.last()
    }

    pub fn last_index(&self) -> Option<usize> {
        self.entries.len().checked_sub(1)
    }

    /// The newest entry `found` accepts, which is what a row is looked up by.
    pub fn position(&self, found: impl FnMut(&Entry) -> bool) -> Option<usize> {
        self.entries.iter().rposition(found)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The rows entry `index` is worth at `width`, measured and kept. The two
    /// things a height depends on besides the entry and the width are handed in
    /// rather than reached for: the frame's clock and the store a picture's
    /// source is measured by.
    pub fn measure(
        &mut self,
        index: usize,
        width: u16,
        now: Duration,
        pictures: Option<&Store>,
    ) -> usize {
        if let Some(rows) = self.heights.at(index, width) {
            return rows;
        }
        let measurement = match &self.entries[index] {
            // An answer still arriving is counted from the cut the last delta
            // left: the rows above it are already counted, and the fence that
            // closed there is neither laid out nor highlighted again.
            Entry::Answer {
                text, done: false, ..
            } => {
                let from = self.heights.cut(index, width);
                let counted = rows::answer_height(text, width, pictures, from);
                Measurement {
                    rows: counted.rows,
                    counted: counted.rows.saturating_sub(from.map_or(0, |cut| cut.above)),
                    cut: Some(counted.cut),
                }
            }
            entry => {
                let rows = rows::entry_height(entry, width, now, pictures);
                Measurement {
                    rows,
                    counted: rows,
                    cut: None,
                }
            }
        };
        self.heights.keep(index, width, measurement);
        measurement.rows
    }

    /// Every height goes: a diagram landed somewhere in the thread, or the
    /// width changed. The two events that move rows no index can name.
    pub fn forget(&mut self) {
        self.heights.forget();
    }

    /// The frame's own cost, which the measure tests read.
    pub fn measured(&self) -> usize {
        self.heights.measured()
    }

    pub fn counted(&self) -> usize {
        self.heights.counted()
    }
}
