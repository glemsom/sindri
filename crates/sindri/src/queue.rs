//! The prompts typed while a run is live, waiting above the composer in the order they
//! were typed and counted in one row: the queue §12 names, and the value `↑` and
//! `Ctrl-X` reach into (A9).
//!
//! Nothing here knows what a terminal is (ADR-0005): a queue is prompts in a line, so
//! the order they run in, the newest one recalled and the row the frame says them in are
//! all functions of what is waiting and are proved by unit tests rather than by an
//! eyeball. A prompt is one entry however many lines it carries — §12's "whole" — and it
//! is never taken apart to be counted, drawn or given back.
//!
//! Who queues a prompt and what happens when one comes back out is not this value's
//! business: the terminal hands a sent prompt to `push` when a run is live, takes the
//! head when that run stops, and the frame takes the newest back to be the draft again.

use std::collections::VecDeque;

use crate::composer::Prompt;

/// What is waiting to run, oldest first: the prompts typed while a run was in flight, in
/// the order the reader typed them (§12).
///
/// A prompt joins the back and leaves the front when its turn comes, which is what "in
/// order" means as code. The newest leaves the back when `↑` recalls it, because the
/// prompt the reader most likely wants back is the one they typed last (A9).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Queue {
    /// The prompts waiting, oldest first.
    waiting: VecDeque<Prompt>,
}

impl Queue {
    /// An empty queue: the state of every frame before a prompt is typed during a run.
    pub fn new() -> Queue {
        Queue::default()
    }

    /// How many prompts are waiting.
    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    /// Whether anything is waiting, which is the fact `↑` and `Ctrl-X` turn on.
    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    /// A prompt sent while a run is live joins the back: the run in flight is not
    /// interrupted, and the prompt keeps its place behind the ones already waiting.
    pub fn push(&mut self, prompt: Prompt) {
        self.waiting.push_back(prompt);
    }

    /// The prompt whose turn it is, oldest first: what a run that just stopped leaves
    /// for the next one. Nothing waiting is no prompt to run.
    pub fn next_to_run(&mut self) -> Option<Prompt> {
        self.waiting.pop_front()
    }

    /// The newest prompt, taken back out of the queue: what `↑` on the composer's first
    /// line recalls (A9), and what `Ctrl-X` takes and throws away. Nothing waiting is
    /// nothing to take.
    pub fn recall(&mut self) -> Option<Prompt> {
        self.waiting.pop_back()
    }

    /// The one row the queue is said in — `3 queued` — and nothing while it holds
    /// nothing, because a frame with no queue is the frame that was there before the
    /// first prompt waited.
    ///
    /// The count and never the prompts: a queued prompt is one the reader typed and can
    /// have back whole with `↑`, so a row of shortened copies above the composer would be
    /// the same words drawn twice, once of them wrong (§12).
    pub fn label(&self) -> Option<String> {
        (!self.waiting.is_empty()).then(|| format!("{} queued", self.waiting.len()))
    }

    /// The rows this queue takes above the composer: the one its label is drawn on, and
    /// none at all while nothing waits.
    pub fn rows(&self) -> u16 {
        u16::from(!self.waiting.is_empty())
    }
}

#[cfg(test)]
mod tests {
    //! The queue's own rules, one test each: the order the prompts run in, the newest
    //! one recalled, and the row the frame says them in. None of them needs a terminal,
    //! which is the point of a value (§12, ADR-0005).

    use super::*;
    use crate::composer::{Composer, Key, Outcome};

    /// A prompt as `Enter` hands one over: the draft a reader typed became something a
    /// run could take, so a test never builds a prompt by a path the product has not.
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
