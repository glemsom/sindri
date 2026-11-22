use std::fs;

use sindri::stream::{Said, Stream};
use sindri_core::provider::Deltas;

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
fn a_turn_that_has_only_reasoned_keeps_one_thought_and_no_answer() {
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
fn the_turn_in_flight_lays_nothing_it_is_drawn_as() {
    let source = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/stream.rs"))
        .expect("the turn in flight's own module");
    for needle in [
        "crate::rows",
        "crate::markdown",
        "crate::mark",
        "crate::pulse",
        "crate::picture",
        "Row",
        "Head",
        "Surface",
    ] {
        let written = source
            .lines()
            .position(|line| line.contains(needle))
            .map(|at| at + 1);
        assert_eq!(
            written, None,
            "the turn in flight keeps the wire's words and lays none of them: `{needle}` is \
             written on line {written:?} of src/stream.rs"
        );
    }
}
