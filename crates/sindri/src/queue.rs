use std::collections::VecDeque;

use crate::composer::Prompt;

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

    pub fn recall(&mut self) -> Option<Prompt> {
        self.waiting.pop_back()
    }

    /// The count and never the prompts, which `↑` hands back whole.
    pub fn label(&self) -> Option<String> {
        (!self.waiting.is_empty()).then(|| format!("{} queued", self.waiting.len()))
    }

    pub fn rows(&self) -> u16 {
        u16::from(!self.waiting.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::{Composer, Key, Outcome};

    fn prompt(text: &str) -> Prompt {
        let mut composer = Composer::new();
        composer.paste(text);
        match composer.key(Key::Enter) {
            Outcome::Send(prompt) => prompt,
            other => panic!("a draft should have been sent, and {other:?} came instead"),
        }
    }

    #[test]
    fn a_prompt_typed_during_a_run_joins_the_back_of_the_queue() {
        let mut queue = Queue::new();
        queue.push(prompt("one"));
        queue.push(prompt("two"));

        assert_eq!(queue.len(), 2);
        assert!(!queue.is_empty());
    }

    #[test]
    fn the_prompts_run_in_the_order_they_were_typed() {
        let mut queue = Queue::new();
        for text in ["first", "second", "third"] {
            queue.push(prompt(text));
        }

        let ran: Vec<String> = std::iter::from_fn(|| queue.next_to_run())
            .map(|prompt| prompt.text().to_owned())
            .collect();

        assert_eq!(ran, ["first", "second", "third"], "in order, oldest first");
        assert!(queue.is_empty(), "and the queue is spent by running it");
    }

    #[test]
    fn the_newest_queued_prompt_is_the_one_recalled() {
        let mut queue = Queue::new();
        for text in ["first", "second", "third"] {
            queue.push(prompt(text));
        }

        assert_eq!(
            queue.recall().as_ref().map(Prompt::text),
            Some("third"),
            "the prompt typed last is the one `↑` takes back"
        );
        assert_eq!(queue.len(), 2, "and the one behind it is next");
    }

    #[test]
    fn a_queue_holding_nothing_runs_nothing_and_recalls_nothing() {
        let mut queue = Queue::new();

        assert_eq!(queue.next_to_run(), None);
        assert_eq!(queue.recall(), None);
        assert_eq!(queue.label(), None, "and has nothing to say on the frame");
    }

    #[test]
    fn the_label_counts_what_is_waiting() {
        let mut queue = Queue::new();
        queue.push(prompt("one"));
        assert_eq!(queue.label().as_deref(), Some("1 queued"));

        queue.push(prompt("two"));
        queue.push(prompt("three"));
        assert_eq!(queue.label().as_deref(), Some("3 queued"));
    }

    #[test]
    fn a_prompt_waits_whole_however_many_lines_it_carries() {
        let mut queue = Queue::new();
        queue.push(prompt("one\ntwo\nthree"));

        assert_eq!(queue.len(), 1, "three lines are one prompt, not three");
        assert_eq!(queue.rows(), 1, "so it takes the one row the count is");
        assert_eq!(
            queue.recall().as_ref().map(Prompt::text),
            Some("one\ntwo\nthree"),
            "and comes back out with every line break it went in with"
        );
    }

    #[test]
    fn the_queue_takes_one_row_and_none_while_it_holds_nothing() {
        let mut queue = Queue::new();
        assert_eq!(
            queue.rows(),
            0,
            "an empty queue is no row above the composer, so the frame is the one it was"
        );

        queue.push(prompt("one"));
        assert_eq!(queue.rows(), 1, "and one row once something waits");

        queue.push(prompt("two"));
        assert_eq!(
            queue.rows(),
            1,
            "however many prompts wait, the count is one row"
        );
    }
}
