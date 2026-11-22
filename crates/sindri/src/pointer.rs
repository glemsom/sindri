use crate::link;
use crate::mark::Cell;

/// Whether a terminal reports a pointer, read off the names it gives itself: the ones that place a
/// hyperlink are the ones a pointer is asked of, because no terminal that draws a link fails to
/// report a press. A terminal that says nothing reports none.
pub fn speaks(term: Option<&str>, program: Option<&str>, vte: Option<&str>) -> bool {
    link::speaks(term, program, vte)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    Wheel(Notch),
    Press(Cell),
}

pub(crate) const ROWS: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notch {
    Up,
    Down,
}

#[cfg(test)]
mod tests;
