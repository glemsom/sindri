//! The turn in flight, held to the value the wire feeds: the runs a turn was said in, in the order
//! they land, and the revision that counts them. The rows they are drawn as belong to `rows`, and
//! the frame that shows the same turn beside the settled entries is `frames`' `streaming-120x40`.

use std::fs;

use sindri::stream::{Said, Stream};
use sindri_core::provider::Deltas;

/// The file and line of every `needle` written in the turn in flight's module, as a search would
/// report them, so a failure says where the copy is and not only that one exists.
fn naming(needle: &str) -> Vec<String> {
    let body = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/stream.rs"))
        .expect("the turn in flight's own module");
    body.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(needle))
        .map(|(at, line)| format!("src/stream.rs:{}: {line}", at + 1))
        .collect()
}

#[test]
fn thought_and_answer_fragments_land_in_the_order_they_arrive() {
    let mut stream = Stream::new();
    stream.reasoning("the fence is the question. ");
    stream.text("A fence still open ");
    stream.reasoning("so its tail is code. ");
    stream.text("is a code block.");

    assert_eq!(
        stream.said().collect::<Vec<_>>(),
        [
            ("the fence is the question. ", Said::Reasoning),
            ("A fence still open ", Said::Answer),
            ("so its tail is code. ", Said::Reasoning),
            ("is a code block.", Said::Answer),
        ],
        "the runs a turn was said in stand in the order the fragments landed"
    );
}

#[test]
fn fragments_of_one_kind_accumulate_into_one_run() {
    let mut stream = Stream::new();
    for fragment in ["A fence ", "still ", "open ", "is code."] {
        stream.text(fragment);
    }

    assert_eq!(
        stream.said().collect::<Vec<_>>(),
        [("A fence still open is code.", Said::Answer)],
        "the fragments of one run are one run"
    );
}

#[test]
fn a_turn_that_has_only_reasoned_has_said_no_answer() {
    let mut stream = Stream::new();
    stream.reasoning("thinking it over. ");
    stream.reasoning("");

    assert_eq!(
        stream.said().collect::<Vec<_>>(),
        [("thinking it over. ", Said::Reasoning)],
        "one thought is one run, and an empty fragment adds a word to none"
    );
    assert_eq!(
        stream.revision(),
        1,
        "the revision counts the fragment that landed, and an empty one is not a tick"
    );
}

#[test]
fn the_turn_in_flight_names_nothing_it_is_drawn_as() {
    for needle in [
        "crate::rows",
        "Row",
        "rows",
        "markdown",
        "gutter",
        "mark",
        "Head",
        "Surface",
    ] {
        let named = naming(needle);
        assert!(
            named.is_empty(),
            "the turn in flight keeps the wire's words and lays none of them: `{needle}` is \
             written at {named:?}"
        );
    }
}
