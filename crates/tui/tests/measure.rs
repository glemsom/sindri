//! What an entry is worth in rows is kept until the entry's text or the width
//! it was measured at changes: a cached height is trusted only for the text it
//! was measured for. A run streams into the tail several times a second, and
//! only the entry the delta landed in may cost the frame a layout.

mod common;

pub use common::app;
use sindri_core::agent::Event;
use sindri_tui::App;
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
