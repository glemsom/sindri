//! What each entry is worth in rows, kept per entry at the width it was
//! measured at. The frame asks the same question every frame, and answering it
//! re-parses markdown and re-highlights every fence in the entry, so a height
//! is remembered — and trusted only for the text it was measured for at the
//! width it was measured at. A change forgets the entries it touched and no
//! others: a run streams into the tail several times a second, and the entries
//! above it did not change. A delta into an answer is counted from the cut the
//! last one left, which is what makes it cost the rows it touched and not the
//! answer it landed in.

use crate::markdown::Cut;

/// The heights of a thread's entries, how many of them were measured from
/// scratch rather than read back, and how many rows the measuring had to read.
#[derive(Debug, Default)]
pub(super) struct Heights {
    rows: Vec<Option<Slot>>,
    measured: usize,
    counted: usize,
}

/// What an entry was measured at: the width it is worth `rows` at, and — for an
/// answer still arriving — the cut its next delta is counted from. The two are
/// forgotten apart: a delta makes the height stale and leaves the cut valid,
/// because the text above the cut is not what changed.
#[derive(Debug, Clone, Copy)]
struct Slot {
    width: u16,
    rows: Option<usize>,
    cut: Option<Cut>,
}

/// One measurement: the rows the entry is worth, the rows that had to be read
/// to learn it, and the cut the next delta into the entry is counted from.
#[derive(Debug, Clone, Copy)]
pub(super) struct Measurement {
    pub rows: usize,
    /// The rows read: the whole entry where it was measured from its first row,
    /// and the block the delta landed in where it was counted from a cut.
    pub counted: usize,
    /// Where a delta landing in the entry next is counted from, where it is an
    /// answer still arriving.
    pub cut: Option<Cut>,
}

impl Heights {
    /// The rows entry `index` was measured at, where it has been measured at
    /// this width: nothing is trusted for a width that changed.
    pub(super) fn at(&self, index: usize, width: u16) -> Option<usize> {
        match *self.rows.get(index)? {
            Some(slot) if slot.width == width => slot.rows,
            _ => None,
        }
    }

    /// The cut entry `index` was counted from at `width`: where the delta that
    /// lands in it next is counted, or nothing where the entry has not been
    /// counted here — a width that changed reads it whole again.
    pub(super) fn cut(&self, index: usize, width: u16) -> Option<Cut> {
        match *self.rows.get(index)? {
            Some(slot) if slot.width == width => slot.cut,
            _ => None,
        }
    }

    /// Keeps what entry `index` was measured to be, and counts the measurement:
    /// the cost of a frame is how many of these it had to make, and how many
    /// rows they read.
    pub(super) fn keep(&mut self, index: usize, width: u16, measured: Measurement) {
        self.measured += 1;
        self.counted += measured.counted;
        if self.rows.len() <= index {
            self.rows.resize(index + 1, None);
        }
        self.rows[index] = Some(Slot {
            width,
            rows: Some(measured.rows),
            cut: measured.cut,
        });
    }

    /// How many entries have been measured since the frame that asked first.
    pub(super) fn measured(&self) -> usize {
        self.measured
    }

    /// How many rows have been read since the frame that asked first: the
    /// figure a streamed delta is charged for, since the answer above it is
    /// read by nobody.
    pub(super) fn counted(&self) -> usize {
        self.counted
    }

    /// Every height is forgotten, and every cut with it: a width that changed or
    /// a thread that moved is not something a cut survives.
    pub(super) fn forget(&mut self) {
        self.rows.fill(None);
    }

    /// The heights from `index` on are forgotten, and the cuts kept: a height is
    /// stale because a delta landed, and the cut is where that delta is counted
    /// from — so what a delta costs is the block it touched and not the answer
    /// above it.
    pub(super) fn forget_from(&mut self, index: usize) {
        if let Some(slots) = self.rows.get_mut(index..) {
            for slot in slots.iter_mut().flatten() {
                slot.rows = None;
            }
        }
    }
}
