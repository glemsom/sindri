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
    /// One notch of the wheel, which moves the transcript the way the page keys move it.
    Wheel(Notch),
    /// A press landed on one cell of the frame.
    Press(Cell),
}

/// The way the wheel turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notch {
    Up,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pointer_is_asked_of_the_terminals_a_hyperlink_is_placed_on_and_of_no_other() {
        assert!(
            speaks(Some("xterm-kitty"), None, None),
            "a terminal that names itself after one that reports a pointer is asked for one"
        );
        assert!(
            speaks(Some("xterm-256color"), Some("WezTerm"), None),
            "and one named by the program that runs it"
        );
        assert!(
            speaks(Some("xterm-256color"), None, Some("6000")),
            "a VTE places a hyperlink and reports a pointer from the version that learned one"
        );
        assert!(
            !speaks(Some("xterm-256color"), None, Some("4900")),
            "and an older one is handed a frame that takes none"
        );
        assert!(
            !speaks(Some("xterm-256color"), None, None),
            "an xterm names no pointer this frame knows how to read"
        );
        assert!(
            !speaks(None, None, None),
            "and neither does a terminal that names nothing"
        );
    }
}
