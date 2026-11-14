//! The conversation is what outlives a run in a launch that holds one. `clear` is the whole of what
//! `/new` owes it: what it holds before that is never joined to the run after it.

use crate::turn::Turn;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversation {
    turns: Vec<Turn>,
}

impl Conversation {
    pub fn new() -> Conversation {
        Conversation::default()
    }

    /// The turns a run joining now is asked after, in the order they ended.
    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    pub fn join(&mut self, kept: Turn) {
        self.turns.push(kept);
    }

    /// `/new`: the conversation starts again, so the run that joins next carries nothing.
    pub fn clear(&mut self) {
        self.turns.clear();
    }
}
