//! The conversation is what outlives a run in a launch that holds one: the prompt of each run that
//! ended beside the answer it got, handed to the run that joins it next, and emptied when the reader
//! starts over.

use crate::run::Exchange;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversation {
    kept: Vec<Exchange>,
}

impl Conversation {
    pub fn new() -> Conversation {
        Conversation::default()
    }

    /// The prompts a run joining now is asked after, in the order they ended.
    pub fn kept(&self) -> &[Exchange] {
        &self.kept
    }

    pub fn len(&self) -> usize {
        self.kept.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kept.is_empty()
    }

    pub fn join(&mut self, kept: Exchange) {
        self.kept.push(kept);
    }

    /// `/new`: the conversation starts again, so the run that joins next carries nothing.
    pub fn clear(&mut self) {
        self.kept.clear();
    }
}
