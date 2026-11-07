//! The turn in flight, held to §4's two promises through the value the wire will feed:
//! text and reasoning stream to the front end as they land, each in its own gutter and
//! its own ink, and the rows a reader has already seen never move under them.
//!
//! What is asserted here is the value a fragment lands in (#43 wires the core's
//! `Deltas` to it), so the picture is held at its rows: a row's words and the ink it is
//! drawn in, one row at a time. The frame that shows the same turn beside the settled
//! entries is `frames`' `streaming-120x40`.

use sindri::ink::Ink;
use sindri::rows::Row;
use sindri::stream::{Said, Stream};
use sindri_core::provider::Deltas;

/// The words a row is drawn with, its gutter and all.
fn words(row: &Row) -> String {
    row.spans().iter().map(|(text, _)| text.as_str()).collect()
}

/// The inks a row's spans are drawn in, in order.
fn inks(row: &Row) -> Vec<Ink> {
    row.spans().iter().map(|(_, ink)| *ink).collect()
}

/// §4 and §12's first promise: reasoning and text land in the order they arrive, and
/// each is drawn in its own gutter and its own ink, so a run that reasons a line,
/// answers a line and reasons on is read as it was said rather than as two piles — the
/// thought first and the answer after it, whatever the order really was.
#[test]
fn thought_and_answer_fragments_land_in_the_order_they_arrive() {
    let mut stream = Stream::new();
    stream.reasoning("the fence is the question. ");
    stream.text("A fence still open ");
    stream.reasoning("so its tail is code. ");
    stream.text("is a code block.");

    let rows = stream.rows(60);
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

    // And the turn settles into the runs it was said in, which is what a run writes
    // down as entries when the turn ends: one fact per run, in this order.
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

/// A streaming answer renders incrementally: a fragment re-wraps the row it lands on and
/// moves no row above it, so an answer grows at the tail and a reader already reading it
/// is never sent back to the top. And a fence still open — the last thing a
/// half-streamed answer is
/// likely to hold — is drawn as code from that line on, never as prose that ate the
/// rest of the answer (ADR-0008; #52 keeps this rule when it replaces the plain
/// rendering).
#[test]
fn a_streaming_answer_adds_rows_and_a_fence_still_open_is_code() {
    let mut stream = Stream::new();
    stream.reasoning("read the fence rule first. ");
    stream.text("The rule is one line: ");
    let drawn = stream.rows(40);
    assert!(!drawn.is_empty(), "a fragment of prose is rows");

    stream.text("a fence still open is a code block, never prose.\n```rust\nfn main() {");
    let more = stream.rows(40);
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
            "  ```rust",
            "  fn main() {"
        ],
        "the fence and its still-streaming contents are drawn as code"
    );
    assert_eq!(
        inks(more.last().expect("a row under the fence")),
        [Ink::Plain, Ink::Muted],
        "and in the ink code is read in, not as prose"
    );
}

/// Two fragments of one kind, one after the other, are one run: a wire sends a word or
/// a few characters at a time, and a run per fragment would be a row per fragment.
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
        stream.rows(40).iter().map(words).collect::<Vec<_>>(),
        ["  A fence still open is code."]
    );
}
