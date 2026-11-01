//! What one frame costs: the rows its **window** holds, and not the rows of the
//! answer that are above or below them. The frame reports what it measured, what
//! it re-measured and — the figure these tests hold — how many rows it built,
//! so a long answer's price is counted rather than timed.

mod common;

use common::{app, press};
use crossterm::event::{KeyCode, KeyModifiers};
use sindri_core::agent::Event;
use sindri_tui::markdown;
use sindri_tui::render::transcript;

/// An answer worth thousands of rows: a hundred fences of prose and code each,
/// which is what an agent answer of real length is made of.
fn answer(fences: usize) -> String {
    let mut text = String::new();
    for index in 0..fences {
        text.push_str(&format!(
            "Block {index} says this about the thing:\n\n```rust\nfn f{index}(x: u32) -> u32 {{\n    let y = x + {index}; // a comment\n    y * 2\n}}\n```\n\nAnd a paragraph of prose under it, long enough that it wraps across the column it is laid out in, more than once at any width a terminal is.\n\n"
        ));
    }
    text
}

/// A frame holding one settled answer of `fences` blocks.
fn answering(fences: usize) -> sindri_tui::App {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: answer(fences),
    });
    app
}

#[test]
fn a_long_answer_costs_the_window_and_not_the_answer() {
    let mut app = answering(400);
    let held = app.measure(0, 120);
    assert!(held > 3_000, "the answer is {held} rows long");

    let view = transcript::window(&mut app, 120, 40);
    assert_eq!(view.plain.len(), 40, "a window is a screen tall");
    assert!(
        view.built <= 40 + 6,
        "a window of forty rows built {} of the answer's {held}",
        view.built
    );
    assert!(
        !view.plain.is_empty() && view.plain.iter().any(|row| row.contains("Block")),
        "and the rows it built are the newest ones: {:?}",
        &view.plain[..view.plain.len().min(4)]
    );
}

#[test]
fn a_delta_costs_the_rows_it_touched_and_not_the_answer_it_landed_in() {
    let mut recounted = Vec::new();
    for fences in [5, 20, 60] {
        let mut app = answering(fences);
        transcript::window(&mut app, 120, 40);
        app.event(Event::TextDelta {
            turn: 1,
            text: "\n\nAnd one more paragraph, which is the delta.\n".to_string(),
        });
        let view = transcript::window(&mut app, 120, 40);
        recounted.push(view.recounted);
    }
    assert!(
        recounted.iter().all(|&rows| (1..=8).contains(&rows)),
        "a delta recounts the rows it landed on and no more: {recounted:?}"
    );
    assert_eq!(
        recounted[0], recounted[2],
        "a delta costs the same at sixty fences as at five: {recounted:?}"
    );
}

#[test]
fn the_tick_of_a_settled_frame_costs_the_window_and_not_the_thread() {
    let mut app = answering(400);
    let first = transcript::window(&mut app, 120, 40);
    let second = transcript::window(&mut app, 120, 40);
    let third = transcript::window(&mut app, 120, 40);

    assert_eq!(second.plain, third.plain, "an unchanged frame is unchanged");
    assert_eq!(
        second.built, third.built,
        "the pulse's tick builds what the window holds and no more"
    );
    assert!(
        second.built <= 40,
        "and a tick costs a window of rows, not the thread: {} rows",
        second.built
    );
    assert!(
        second.built <= first.built,
        "a settled frame is never dearer than the one that settled it: {} after {}",
        second.built,
        first.built
    );
}

#[test]
fn a_window_high_in_a_long_answer_costs_the_window_too() {
    let mut app = answering(400);
    transcript::window(&mut app, 120, 40);
    press(&mut app, KeyCode::Home, KeyModifiers::NONE);
    let top = transcript::window(&mut app, 120, 40);
    assert!(
        top.plain.iter().take(2).any(|row| row.contains("Block 0")),
        "Home is the first turn: {:?}",
        &top.plain[..2]
    );
    assert!(
        top.built <= 40 + 6,
        "the first screen of a long answer built {} rows",
        top.built
    );
}

#[test]
fn a_resize_lays_the_window_out_at_the_new_width_and_costs_no_more_than_it() {
    let mut app = answering(400);
    transcript::window(&mut app, 120, 40);
    let narrow = transcript::window(&mut app, 80, 40);
    assert!(
        narrow.built <= 40 + 6,
        "the window at the new width built {} rows",
        narrow.built
    );

    let mut fresh = answering(400);
    let same = transcript::window(&mut fresh, 80, 40);
    assert_eq!(
        narrow.plain, same.plain,
        "a resize lays the window out as the width always would have"
    );
}

/// A document of everything an answer is made of, for the exactness the window
/// owes a reader: a row is the row it was whichever rows of the answer are asked
/// for.
const DOCUMENT: &str = "# A heading\n\nA paragraph that wraps more than once at a narrow width, because it goes on and on about the thing it is about.\n\n- a list item\n- another item, which also wraps at a narrow width\n\n> a quote of a thing someone said\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n```\nplain fence\nwith two lines\n```\n\n## Another heading\n\nLast words.\n";

#[test]
fn the_rows_a_window_asks_for_are_the_rows_the_whole_answer_has() {
    for width in [20, 40, 61, 120] {
        let whole = markdown::lines(DOCUMENT, width, None);
        let total = markdown::height(DOCUMENT, width, None);
        assert_eq!(
            total,
            whole.rows.len(),
            "the count a height answers is the rows the whole layout made, at {width} columns"
        );
        for from in (0..whole.rows.len()).step_by(3) {
            for want in [1, 7, 40] {
                let cut = markdown::lines_in(DOCUMENT, width, None, from..from + want);
                let to = (from + want).min(whole.rows.len());
                let mine: Vec<String> = cut.rows.iter().map(text).collect();
                let theirs: Vec<String> = whole.rows[from..to].iter().map(text).collect();
                assert_eq!(
                    mine, theirs,
                    "rows {from}..{to} of a {width}-column answer, asking for {want} of them"
                );
                assert_eq!(
                    cut.total,
                    whole.rows.len(),
                    "and the whole answer is counted"
                );
            }
        }
    }
}

#[test]
fn an_entry_worth_in_rows_is_the_answer_row_for_row() {
    for width in [20, 40, 120] {
        let mut app = answering(3);
        // The marker row the answer hangs under, and the answer's own rows.
        assert_eq!(
            app.measure(0, width),
            1 + markdown::lines(&answer(3), width.saturating_sub(2), None)
                .rows
                .len(),
            "the row count an entry is worth at {width} columns"
        );
    }
}

fn text(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_string()
}
