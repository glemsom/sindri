use std::collections::VecDeque;

use crate::composer::{self, Prompt};

/// The rows the queue takes of the composer's band at most.
pub const MAX_ROWS: u16 = composer::MAX_ROWS - composer::MIN_ROWS;

/// What the band draws of the queue in the rows it gives it.
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

    pub fn recall(&mut self) -> Option<Prompt> {
        self.waiting.pop_back()
    }

    /// The count and never the prompts, which `↑` hands back whole.
    pub fn label(&self) -> Option<String> {
        (!self.waiting.is_empty()).then(|| format!("{} queued", self.waiting.len()))
    }

    pub fn shown(&self, room: u16) -> Shown {
        let room = usize::from(room);
        let len = self.waiting.len();
        let drawn = match len > room {
            true => room.saturating_sub(1),
            false => len,
        };
        Shown {
            kept_back: (room > 0 && len > drawn).then(|| len - drawn),
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

/// The one row a prompt is drawn as, however many lines it was typed in.
fn first_line(prompt: &Prompt) -> String {
    prompt.text().lines().next().unwrap_or_default().to_owned()
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
        assert_eq!(
            queue.rows(),
            1,
            "so it takes the one row its own first line is"
        );
        assert_eq!(
            queue.recall().as_ref().map(Prompt::text),
            Some("one\ntwo\nthree"),
            "and comes back out with every line break it went in with"
        );
    }

    #[test]
    fn a_band_holding_the_whole_queue_draws_a_prompt_a_row_as_its_first_line() {
        let mut queue = Queue::new();
        for text in ["one\ntwo", "three", "four"] {
            queue.push(prompt(text));
        }

        assert_eq!(
            queue.shown(3),
            Shown {
                kept_back: None,
                prompts: vec![
                    String::from("one"),
                    String::from("three"),
                    String::from("four"),
                ],
            },
            "a prompt is one row whatever lines it holds, the newest at the foot and nothing \
             counted back"
        );
        assert_eq!(
            queue.shown(MAX_ROWS),
            queue.shown(3),
            "and a band with rows to spare draws no blank row of its own"
        );
    }

    #[test]
    fn a_band_shorter_than_the_queue_draws_the_newest_it_holds_and_counts_the_rest() {
        let mut queue = Queue::new();
        for text in ["one", "two", "three", "four", "five"] {
            queue.push(prompt(text));
        }

        assert_eq!(
            queue.shown(3),
            Shown {
                kept_back: Some(3),
                prompts: vec![String::from("four"), String::from("five")],
            },
            "the band's first row goes to the count of the three older prompts, and the two the \
             reader typed last stand under it"
        );
    }

    #[test]
    fn a_band_with_no_row_for_the_queue_draws_nothing_of_it() {
        let mut queue = Queue::new();
        for text in ["one", "two"] {
            queue.push(prompt(text));
        }

        assert_eq!(
            queue.shown(0),
            Shown::default(),
            "a band of no rows has no row to draw a prompt in and none to say a count in, and the \
             footer keeps the depth beside the key that recalls one"
        );
    }

    #[test]
    fn nothing_waiting_is_no_row_drawn_and_nothing_counted_back() {
        assert_eq!(Queue::new().shown(MAX_ROWS), Shown::default());
    }

    #[test]
    fn every_row_the_queue_draws_is_a_row_of_the_band_it_was_given() {
        for len in 0..=MAX_ROWS + 4 {
            let mut queue = Queue::new();
            for at in 0..len {
                queue.push(prompt(&at.to_string()));
            }
            for room in 0..=MAX_ROWS {
                let shown = queue.shown(room);
                assert!(
                    shown.prompts.len() + usize::from(shown.kept_back.is_some())
                        <= usize::from(room),
                    "{len} prompts drawn in {room} rows take more rows than the band gave them"
                );
            }
        }
    }

    #[test]
    fn the_band_is_asked_for_a_row_a_prompt_and_none_while_nothing_waits() {
        let mut queue = Queue::new();
        assert_eq!(
            queue.rows(),
            0,
            "an empty queue asks for no row, so the transcript keeps every row it would have"
        );

        for text in ["one", "two", "three"] {
            queue.push(prompt(text));
        }
        assert_eq!(queue.rows(), 3, "three prompts are three rows");

        for text in ["four", "five"] {
            queue.push(prompt(text));
        }
        assert_eq!(
            queue.rows(),
            MAX_ROWS,
            "and however deep the queue, it asks for no more than the band has to give it"
        );
    }
}
