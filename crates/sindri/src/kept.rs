//! The list a screen keeps: what a cursor movement walks, what a typed glyph reaches and which row a
//! press chooses — the picker's files, the wire's served names and the config's keys answering alike.

use crate::composer::Key;

/// A step of a list's mark, which `↑` and `↓` are on every screen that keeps one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Previous,
    Next,
}

/// The three lists a screen keeps, named by the entry that describes the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Files,
    Models,
    Settings,
}

/// The rows a screen keeps, as the frame asks them: the mark a movement moves, the query a typed
/// glyph reaches, and the row a press chooses — one answer each, so a screen that gains an action is
/// found in its description and nowhere else.
pub trait Kept {
    /// The mark one step along, and whether it moved.
    fn step(&mut self, step: Step) -> bool;
    /// One typed glyph, and whether the rows changed.
    fn typed(&mut self, key: Key) -> bool;
    /// The mark moves to the row a press landed on, and whether the list still holds it.
    fn press(&mut self, at: usize) -> bool;
}
