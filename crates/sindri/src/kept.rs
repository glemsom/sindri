//! The list a screen keeps: what a cursor movement walks, what a typed glyph reaches and which row a
//! press chooses — the picker's files, the wire's served names, the config's keys and the commands
//! card a draft opens answering alike.

use crate::composer::Key;

/// The way a list's mark moves, which `↑` and `↓` are on every screen that keeps one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Previous,
    Next,
}

/// The rows a screen keeps, as the frame asks them: the picker's files, the wire's served names, the
/// config's keys and the card a draft opens. One answer each, so a screen that gains an action is
/// found in its description and nowhere else.
pub trait Kept {
    /// The mark a way along, and whether it moved.
    fn walked(&mut self, way: Move) -> bool;
    /// A typed glyph or an erasure, and whether the rows changed.
    fn typed(&mut self, key: Key) -> bool;
    /// The row a press landed on, marked, and whether the list still holds it.
    fn pressed(&mut self, at: usize) -> bool;
}
