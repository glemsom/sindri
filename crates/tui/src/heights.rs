//! What each entry is worth in rows, kept per entry at the width it was
//! measured at. The frame asks the same question every frame, and answering it
//! re-parses markdown and re-highlights every fence in the entry, so a height
//! is remembered — and trusted only for the text it was measured for at the
//! width it was measured at. A change forgets the entries it touched and no
//! others: a run streams into the tail several times a second, and the entries
//! above it did not change.

use crate::app::App;

/// The heights of a thread's entries, and how many of them were measured from
/// scratch rather than read back.
#[derive(Debug, Default)]
pub struct Heights {
    rows: Vec<Option<(u16, usize)>>,
    measured: usize,
}

impl Heights {
    /// The rows entry `index` was measured at, where it has been measured at
    /// this width: nothing is trusted for a width that changed.
    pub fn at(&self, index: usize, width: u16) -> Option<usize> {
        match *self.rows.get(index)? {
            Some((measured, rows)) if measured == width => Some(rows),
            _ => None,
        }
    }

    /// Keeps `rows` as what entry `index` is worth at `width`, and counts the
    /// measurement: the cost of a frame is how many of these it had to make.
    pub fn keep(&mut self, index: usize, width: u16, rows: usize) {
        self.measured += 1;
        if self.rows.len() <= index {
            self.rows.resize(index + 1, None);
        }
        self.rows[index] = Some((width, rows));
    }

    /// How many entries have been measured since the frame that asked first.
    pub fn measured(&self) -> usize {
        self.measured
    }

    /// Every height is forgotten.
    fn forget(&mut self) {
        self.rows.fill(None);
    }

    /// The heights from `index` on are forgotten.
    fn forget_from(&mut self, index: usize) {
        if let Some(slots) = self.rows.get_mut(index..) {
            slots.fill(None);
        }
    }
}

impl App {
    /// Every cached height goes: the thread changed in more than one place, or
    /// somewhere that is not its tail.
    pub(crate) fn dirty(&mut self) {
        self.heights.forget();
    }

    /// The heights from `index` on go. A cached height is trusted only for the
    /// text it was measured for, so an entry that changed is forgotten — and
    /// the entries after it with it, which are still right and are the price of
    /// naming one index rather than walking the thread.
    pub(crate) fn dirty_from(&mut self, index: usize) {
        self.heights.forget_from(index);
    }

    /// Only the newest entry changed: what a streamed delta, a line of a call's
    /// output and a prompt just sent all know about themselves, and the reason
    /// a run's fragments cost the entries above them no layout at all.
    pub(crate) fn dirty_tail(&mut self) {
        if let Some(newest) = self.entries.len().checked_sub(1) {
            self.heights.forget_from(newest);
        }
    }
}
