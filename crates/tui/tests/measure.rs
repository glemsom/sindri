//! What an entry is worth in rows is kept until the entry's text or the width
//! it was measured at changes: a cached height is trusted only for the text it
//! was measured for. A run streams into the tail several times a second, and
//! only the entry the delta landed in may cost the frame a layout.

mod common;

pub use common::app;
use sindri_core::agent::Event;
use sindri_tui::App;
use sindri_tui::markdown;
use sindri_tui::render::transcript;

/// A question and the answer streaming under it: two entries, the second the
/// one a delta lands in.
fn asked() -> App {
    let mut app = app();
    app.note_info("what does a turn cost?");
    app.event(Event::TextDelta {
        turn: 1,
        text: "a round trip".to_string(),
    });
    app
}

#[test]
fn a_streamed_delta_measures_the_tail_entry_and_not_the_one_above_it() {
    let mut app = asked();
    let laid = transcript::window(&mut app, 80, 20);
    assert_eq!(laid.remeasured, 2, "two entries, each measured once");

    app.event(Event::TextDelta {
        turn: 1,
        text: ", and a measurement".to_string(),
    });
    let streamed = transcript::window(&mut app, 80, 20);
    assert_eq!(
        streamed.remeasured, 1,
        "the delta is measured into the entry it landed in, and the entry above it \
         is laid out from the height it already had"
    );
    assert!(
        streamed
            .plain
            .iter()
            .any(|row| row.contains("a measurement")),
        "and the tail's own rows were laid out again, so the delta is on screen: {:?}",
        streamed.plain
    );
}

/// An answer of everything an answer is made of, sent a fragment at a time: a
/// wrapped paragraph, a fence opening, the fence closing, a table, a heading.
const STREAMED: &str = "# A heading\n\nA paragraph long enough that it wraps more than \
once at a narrow column, because it goes on about the thing it is about.\n\n\
```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n\
| a | b |\n|---|---|\n| 1 | 2 |\n\nAnd the last words.\n";

/// `text` in fragments of `size` characters, as a stream arrives: inside a word,
/// inside a fence, and on the blank lines between blocks.
fn fragments(text: &str, size: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(size)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

#[test]
fn a_streamed_answer_is_worth_exactly_the_rows_it_has_said() {
    for width in [40, 80] {
        let mut app = app();
        let mut said = String::new();
        for fragment in fragments(STREAMED, 11) {
            said.push_str(&fragment);
            app.event(Event::TextDelta {
                turn: 1,
                text: fragment,
            });
            // The marker row the answer hangs under, and the answer's own rows
            // laid out whole: what the entry is worth, however it was counted.
            let whole = 1 + markdown::lines(&said, width - 2, None).rows.len();
            assert_eq!(
                app.measure(0, width),
                whole,
                "an answer of {said:?} at {width} columns"
            );
        }
        assert_eq!(
            app.measure(0, width),
            1 + markdown::lines(STREAMED, width - 2, None).rows.len(),
            "and the whole answer is worth what the whole answer says"
        );
        // A width the answer never streamed at has no count to resume from: it
        // is read whole, and it is still exactly the rows the answer says.
        let elsewhere = width + 20;
        assert_eq!(
            app.measure(0, elsewhere),
            1 + markdown::lines(STREAMED, elsewhere - 2, None).rows.len(),
            "an answer measured at {elsewhere} columns, which it never streamed at"
        );
    }
}

#[test]
fn a_height_is_only_trusted_at_the_width_it_was_measured_at() {
    let mut app = asked();
    let wide = transcript::window(&mut app, 80, 20);
    assert_eq!(wide.remeasured, 2, "two entries, each measured once");

    let narrow = transcript::window(&mut app, 40, 20);
    assert_eq!(
        narrow.remeasured, 2,
        "the same text at a narrower width is not the text the heights were measured for"
    );
}
