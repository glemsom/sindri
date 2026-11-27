use std::collections::VecDeque;

use crate::composer::Prompt;

pub const MAX_ROWS: u16 = 4;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub kept_back: Option<usize>,
    pub prompts: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Queue {
    waiting: VecDeque<Prompt>,
}

impl Queue {
    pub fn new() -> Queue {
        Queue::default()
    }

    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    pub fn push(&mut self, prompt: Prompt) {
        self.waiting.push_back(prompt);
    }

    pub fn next_to_run(&mut self) -> Option<Prompt> {
        self.waiting.pop_front()
    }

    pub fn take_newest(&mut self) -> Option<Prompt> {
        self.waiting.pop_back()
    }

    pub fn label(&self) -> Option<String> {
        (!self.waiting.is_empty()).then(|| format!("{} queued", self.waiting.len()))
    }

    pub fn shown(&self, room: u16) -> Shown {
        let room = usize::from(room);
        let len = self.waiting.len();
        let counted = len > room && room > 1;
        let drawn = match counted {
            true => room - 1,
            false => len.min(room),
        };
        Shown {
            kept_back: counted.then(|| len - drawn),
            prompts: self
                .waiting
                .iter()
                .skip(len - drawn)
                .map(first_line)
                .collect(),
        }
    }

    pub fn rows(&self) -> u16 {
        u16::try_from(self.waiting.len())
            .unwrap_or(u16::MAX)
            .min(MAX_ROWS)
    }
}

fn first_line(prompt: &Prompt) -> String {
    prompt.text().lines().next().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests;
