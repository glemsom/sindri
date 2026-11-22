use crate::turn::Turn;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversation {
    turns: Vec<Turn>,
}

impl Conversation {
    pub fn new() -> Conversation {
        Conversation::default()
    }

    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    pub fn join(&mut self, kept: Turn) {
        self.turns.push(kept);
    }

    pub fn clear(&mut self) {
        self.turns.clear();
    }
}
