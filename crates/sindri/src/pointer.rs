//! The pointer a frame takes: the wheel's notch and the cell a press landed on, and whether the
//! terminal this frame is drawn on reports one at all.

use crate::link;
use crate::mark::Cell;

/// Whether a terminal reports a pointer, read off the names it gives itself: the ones that place a
/// hyperlink are the ones a pointer is asked of, because no terminal that draws a link fails to
/// report a press. A terminal that says nothing reports none.
pub fn speaks(term: Option<&str>, program: Option<&str>, vte: Option<&str>) -> bool {
    link::speaks(term, program, vte)
}

/// What the pointer did, as one input to a frame: never a key, because a notch moves and a
/// press chooses and neither is typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    /// One notch of the wheel, which moves the transcript three rows: the notch's measure, where a
    /// page key moves it a band.
    Wheel(Notch),
    /// A press landed on one cell of the frame.
    Press(Cell),
}

/// How many rows of the transcript one notch of the wheel moves.
pub(crate) const ROWS: u32 = 3;

/// The way the wheel turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notch {
    Up,
    Down,
}

#[cfg(test)]
mod tests;
