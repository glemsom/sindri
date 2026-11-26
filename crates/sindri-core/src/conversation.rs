use crate::turn::Turn;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversation {
    turns: Vec<Turn>,
    generation: u64,
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

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn join_for(&mut self, generation: u64, kept: Turn) {
        if generation == self.generation {
            self.join(kept);
        }
    }

    pub fn reset(&mut self, loaded: &crate::run::Loaded) {
        self.clear();
        loaded.clear();
    }

    pub fn clear(&mut self) {
        self.turns.clear();
        self.generation = self
            .generation
            .checked_add(1)
            .expect("conversation generation exhausted");
    }
}
