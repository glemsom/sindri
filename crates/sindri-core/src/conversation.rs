//! The conversation is what outlives a run in a launch that holds one. `clear` is the whole of what
//! `/new` owes it: what it holds before that is never joined to the run after it.

use crate::run::Exchange;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversation {
    exchanges: Vec<Exchange>,
}

impl Conversation {
    pub fn new() -> Conversation {
        Conversation::default()
    }

    /// The prompts a run joining now is asked after, in the order they ended.
    pub fn exchanges(&self) -> &[Exchange] {
        &self.exchanges
    }

    pub fn join(&mut self, kept: Exchange) {
        self.exchanges.push(kept);
    }

    /// `/new`: the conversation starts again, so the run that joins next carries nothing.
    pub fn clear(&mut self) {
        self.exchanges.clear();
    }
}
