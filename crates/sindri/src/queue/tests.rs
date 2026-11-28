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
fn a_band_of_one_row_draws_the_newest_prompt_and_never_only_a_count() {
    let mut queue = Queue::new();
    for text in ["one", "two", "three"] {
        queue.push(prompt(text));
    }

    assert_eq!(
        queue.shown(1),
        Shown {
            kept_back: None,
            prompts: vec![String::from("three")],
        },
        "the one row a squeezed band has goes to the prompt the reader typed last, and never \
             to a number standing where a thought should be"
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
                shown.prompts.len() + usize::from(shown.kept_back.is_some()) <= usize::from(room),
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
        "and however deep the queue, it asks for no more than its ceiling, so the transcript \
             keeps every row past it"
    );
}
