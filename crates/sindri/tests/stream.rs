//! The turn in flight, held to the value the wire feeds: text and reasoning stream as they land, each in its own
//! gutter and ink, and the rows a reader has already seen never move under them. The frame that shows the
//! same turn beside the settled entries is `frames`' `streaming-120x40`.

use sindri::ink::Ink;
use sindri::pulse::Pulse;
use sindri::rows::Row;
use sindri::stream::{Said, Stream};
use sindri_core::provider::Deltas;

fn words(row: &Row) -> String {
    row.spans().iter().map(|(text, _)| text.as_str()).collect()
}

fn inks(row: &Row) -> Vec<Ink> {
    row.spans().iter().map(|(_, look)| look.ink()).collect()
}

#[test]
fn thought_and_answer_fragments_land_in_the_order_they_arrive() {
    let mut stream = Stream::new();
    stream.reasoning("the fence is the question. ");
    stream.text("A fence still open ");
    stream.reasoning("so its tail is code. ");
    stream.text("is a code block.");

    let rows = stream.rows(60, None);
    let drawn: Vec<String> = rows.iter().map(words).collect();
    assert_eq!(
        drawn,
        [
            "│ the fence is the question.",
            "  A fence still open",
            "│ so its tail is code.",
            "  is a code block.",
        ],
        "the fragments are drawn in the order they landed, each under its own gutter"
    );
    assert_eq!(
        rows.iter().map(inks).collect::<Vec<_>>(),
        [
            vec![Ink::Dim, Ink::Muted],
            vec![Ink::Plain, Ink::Plain],
            vec![Ink::Dim, Ink::Muted],
            vec![Ink::Plain, Ink::Plain],
        ],
        "a thought is read in reasoning's ink, inside its rule; an answer in the frame's"
    );

    assert_eq!(
        stream.said().collect::<Vec<_>>(),
        [
            ("the fence is the question. ", Said::Reasoning),
            ("A fence still open ", Said::Answer),
            ("so its tail is code. ", Said::Reasoning),
            ("is a code block.", Said::Answer),
        ],
        "the stream keeps the runs a turn was said in, and nothing it can be drawn as"
    );
}

#[test]
fn a_streaming_answer_adds_rows_and_a_fence_still_open_is_code() {
    let mut stream = Stream::new();
    stream.reasoning("read the fence rule first. ");
    stream.text("The rule is one line: ");
    let drawn = stream.rows(40, None);
    assert!(!drawn.is_empty(), "a fragment of prose is rows");

    stream.text("a fence still open is a code block, never prose.\n```rust\nfn main() {");
    let more = stream.rows(40, None);
    assert_eq!(
        &more[..drawn.len() - 1],
        &drawn[..drawn.len() - 1],
        "every row above the one the fragment landed on stands still"
    );
    assert!(
        more.len() > drawn.len(),
        "the answer grew rows as it streamed: {} then {}",
        drawn.len(),
        more.len()
    );
    let tail: Vec<String> = more[drawn.len()..].iter().map(words).collect();
    assert_eq!(
        tail,
        [
            "  open is a code block, never prose.",
            "",
            "  ▏ rust",
            "  ▏ fn main() {"
        ],
        "the fence and its still-streaming contents are drawn as the block they opened"
    );
    assert_eq!(
        inks(more.last().expect("a row under the fence"))[..2],
        [Ink::Plain, Ink::Dim],
        "and behind the bar a code block is set behind, not as prose"
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
    assert_eq!(
        stream.rows(40, None).iter().map(words).collect::<Vec<_>>(),
        ["  A fence still open is code."]
    );
}

#[test]
fn the_marker_heads_the_newest_answers_first_row_and_no_other() {
    let mut stream = Stream::new();
    stream.text("the first thing said. ");
    stream.reasoning("and then a thought. ");
    stream.text("and the answer being read now.");

    let head = Pulse::live(4_200).head();
    let drawn: Vec<String> = stream.rows(60, Some(&head)).iter().map(words).collect();

    assert_eq!(
        drawn,
        [
            "  the first thing said.",
            "│ and then a thought.",
            "⠹ 4.2s and the answer being read now.",
        ],
        "the marker is on the newest answer, and the older one is drawn as it always was"
    );
}

#[test]
fn a_turn_that_has_only_reasoned_opens_the_answers_row_under_its_thought() {
    let mut stream = Stream::new();
    stream.reasoning("thinking it over.");

    let head = Pulse::live(4_200).head();
    let drawn: Vec<String> = stream.rows(60, Some(&head)).iter().map(words).collect();

    assert_eq!(drawn, ["│ thinking it over.", "⠹ 4.2s"]);
}
