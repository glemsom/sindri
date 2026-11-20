//! The mark's contract with the clipboard, proved where a reader's eyes are: a drag on a headless screen, the
//! frame painted from it, the cells the highlight covers read back out of that frame, and those bytes held
//! to what `Ctrl-Y` hands out. A snapshot would compare the frame with a copy of itself.

mod caps; // the terminals these frames are drawn on
mod frame; // one paint-and-read for every test

use frame::Frame;

use ratatui::layout::Rect;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::ink::Ink;
use sindri::mark::Cell;
use sindri::view::{Layout, Transcript, View};
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

const SCREEN: (u16, u16) = (100, 30);

/// The newer answer as the model wrote it: what a copy with no mark takes, written out once and compared whole.
const NEWEST: &str = "A width change drops what the cache held and reflows on the next \
                      paint, once, because a paint lays an entry out to draw it and then \
                      keeps the answer.";

fn transcript() -> Vec<Entry> {
    vec![
        Entry::Turn { turn: 1 },
        answer(
            1,
            "Rendering lives in the front end and not in the core, because an entry is a \
             fact and never a rendering: the terminal UI and any reader draw one run from \
             one value, and the core may not name a terminal at all.",
        ),
        Entry::Turn { turn: 2 },
        answer(2, NEWEST),
    ]
}

fn answer(turn: u32, text: &str) -> Entry {
    Entry::Assistant {
        turn,
        text: SecretSet::default().redact(text),
    }
}

/// The frame the pass laid out, which a pass that drew a frame always hands back.
fn laid_of(frame: &Frame) -> Layout {
    frame
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out")
}

/// The cells the highlight covers, read off the rows the transcript was given.
fn highlighted(frame: &Frame, area: Rect) -> String {
    let mut rows: Vec<String> = Vec::new();
    for y in area.y..area.bottom() {
        let mut line = String::new();
        for (x, glyph) in (area.x..area.right()).zip(frame.row(y).chars()) {
            if frame.ink(x, y) != Some(Ink::Mark) {
                // The mark covers one run of cells a row, so the first cell of it that is not marked
                // ends the run this row carries.
                if !line.is_empty() {
                    break;
                }
                continue;
            }
            line.push(glyph);
        }
        if !line.is_empty() {
            rows.push(line);
        }
    }
    rows.join("\n")
}

/// What `Ctrl-Y` hands the terminal, read after the frame the key changed has been painted.
fn copied(view: &mut View) -> String {
    frame::painted(view, SCREEN.0, SCREEN.1);
    match view.key(Key::Copy) {
        Outcome::Copy(words) => words,
        other => panic!("Ctrl-Y is a copy, and this frame answered it {other:?}"),
    }
}

#[test]
fn what_the_highlight_covers_is_byte_for_byte_what_the_copy_takes() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(transcript()),
    );
    view.drag(Cell::new(1, 2), Cell::new(6, 80));

    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let marked = highlighted(&frame, laid.transcript);
    let taken = copied(&mut view);

    assert!(
        !taken.is_empty(),
        "the drag covered words, or there is nothing to hold the copy to: {:?}",
        frame.cells()
    );
    assert_eq!(
        marked, taken,
        "the cells the highlight covers are the bytes the copy hands out, which is the \
         \"a drag marks exactly the cells the copy will take\" as a comparison rather than \
         a promise"
    );
}

#[test]
fn a_redraw_keeps_the_highlight_and_ctrl_y_copies_it_again() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(transcript()),
    );
    view.drag(Cell::new(1, 2), Cell::new(4, 60));

    let before = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&before);
    let taken = copied(&mut view);
    let after = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let repainted = laid_of(&after);
    let again = copied(&mut view);

    assert!(
        !taken.is_empty(),
        "there was a mark to copy: {:?}",
        before.cells()
    );
    assert_eq!(
        highlighted(&after, repainted.transcript),
        highlighted(&before, laid.transcript),
        "a redraw keeps the highlight, because the mark is corners of the band and not a \
         picture of a frame that has been thrown away"
    );
    assert_eq!(
        again, taken,
        "and `Ctrl-Y` copies again with no fresh selection: the mark outlives the paint \
         that drew it"
    );
}

#[test]
fn a_copy_of_a_row_the_band_cut_hands_back_the_line_as_the_model_wrote_it() {
    let line = "let a_fence_line_wider_than_the_band_it_is_drawn_in_is_kept_in_the_shape_the_model_\
                wrote_it_in_and_no_band_reflows_one = 1;";
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([answer(1, &format!("```rust\n{line}\n```"))]),
    );
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let row = frame.row_holding(laid.transcript, "a_fence_line");
    view.drag(Cell::new(row, 2), Cell::new(row, 40));
    let taken = copied(&mut view);

    assert_eq!(
        taken, line,
        "the words past the mark's right edge are still the row's, so a copy takes the line the \
         model wrote and never the fragment the band's edge left showing"
    );
    assert!(
        !taken.contains('…'),
        "and the cut that says more stands past the edge is no cell of the row, so no copy takes \
         it: {taken:?}"
    );
}

#[test]
fn a_copy_of_a_marked_fence_hands_back_the_fence_whole() {
    let fence = "```rust\nfn one() {\n    let two = 2;\n}\n```";
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([answer(1, fence)]),
    );
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let row = frame.row_holding(laid.transcript, "two = 2");
    view.drag(Cell::new(row, 2), Cell::new(row, 20));
    let taken = copied(&mut view);

    assert_eq!(
        taken, "fn one() {\n    let two = 2;\n}",
        "the mark stands on one line of the fence and the copy takes the fence whole: the line \
         breaks the model wrote, and neither the bar nor the guide nor the cut the frame drew"
    );
}

#[test]
fn a_copy_of_a_fence_the_band_cut_hands_back_every_whole_line_of_it() {
    let long = "let a_fence_line_wider_than_the_band_it_is_drawn_in_is_kept_in_the_shape_the_model_\
                wrote_it_in_and_no_band_reflows_one = 1;";
    let fence = format!("```rust\n{long}\nlet short = 2;\n```");
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([answer(1, &fence)]),
    );
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let row = frame.row_holding(laid.transcript, "short");
    view.drag(Cell::new(row, 2), Cell::new(row, 40));
    let taken = copied(&mut view);

    assert_eq!(
        taken,
        format!("{long}\nlet short = 2;"),
        "the band cut the first line of the fence and drew an ellipsis over its last cell, and the \
         copy is still both lines as the model wrote them: {taken:?}"
    );
}

#[test]
fn a_blank_line_inside_a_marked_fence_is_a_line_of_the_copy() {
    let fence = "```rust\nlet a = 1;\n\nlet b = 2;\n```";
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([answer(1, fence)]),
    );
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let row = frame.row_holding(laid.transcript, "b = 2");
    view.drag(Cell::new(row, 2), Cell::new(row, 20));
    let taken = copied(&mut view);

    assert_eq!(
        taken, "let a = 1;\n\nlet b = 2;",
        "the blank line the model wrote is one line of the fence and one line of the copy: {taken:?}"
    );
}

#[test]
fn the_cells_a_marked_fence_is_highlighted_in_are_its_own_rows() {
    let fence = "```rust\nfn one() {\n    let two = 2;\n}\n```";
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([answer(1, fence)]),
    );
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let row = frame.row_holding(laid.transcript, "two = 2");
    view.drag(Cell::new(row, 2), Cell::new(row, 20));
    let marked = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let lit = highlighted(&marked, laid.transcript);

    for line in ["fn one()", "two = 2"] {
        assert!(
            lit.contains(line),
            "the fence leaves whole, so the rows of it light up with the row the drag stands on, \
             and {line:?} is one of them: {lit:?}"
        );
    }
}

#[test]
fn a_copy_of_a_fence_is_no_other_fence_of_the_same_answer() {
    let text = "```rust\nlet body = 1;\n```\n\nA claim[^1]\n\n[^1]: the note\n\n    ```rust\n    let foot = 2;\n    ```";
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([answer(1, text)]),
    );
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let row = frame.row_holding(laid.transcript, "body = 1");
    view.drag(Cell::new(row, 2), Cell::new(row, 20));
    let taken = copied(&mut view);

    assert_eq!(
        taken, "let body = 1;",
        "the fence at the foot of the answer is a block of its own, and a copy of the one above \
         it is no line of the other: {taken:?}"
    );
}

#[test]
fn a_copy_of_a_fence_marked_on_its_name_hands_back_the_fence_whole() {
    let fence = "```rust crates/sindri/src/rows.rs\nfn one() {}\n```";
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([answer(1, fence)]),
    );
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let row = frame.row_holding(laid.transcript, "rust crates/sindri/src/rows.rs");
    view.drag(Cell::new(row, 0), Cell::new(row, 30));
    let taken = copied(&mut view);

    assert_eq!(
        taken, "fn one() {}",
        "the row that names the fence is one of its rows, so a mark on it takes the fence whole \
         and never the bar the name is drawn behind"
    );
}

#[test]
fn a_mark_that_runs_past_the_fence_is_copied_as_it_is_drawn() {
    let text = "```rust\nlet one = 1;\n```\n\nthe prose under the fence";
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([answer(1, text)]),
    );
    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let laid = laid_of(&frame);
    let row = frame.row_holding(laid.transcript, "let one");
    view.drag(Cell::new(row, 2), Cell::new(row + 2, 30));
    let taken = copied(&mut view);

    assert!(
        taken.contains("the prose under the fence"),
        "a mark that runs past the fence covers the prose too, and a copy of it keeps every row \
         the drag covered: {taken:?}"
    );
}

#[test]
fn ctrl_y_with_no_mark_copies_the_last_answer() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(transcript()),
    );

    let taken = copied(&mut view);

    assert_eq!(
        taken, NEWEST,
        "with nothing marked, `Ctrl-Y` copies the newest answer whole — the line breaks \
         the model wrote, and not the ones the band wrapped it into"
    );
}

#[test]
fn ctrl_y_copies_a_struck_run_with_the_tildes_the_model_wrote() {
    let struck = "A ~~struck~~ word is drawn crossed out and still copied `~~struck~~`, \
                  because a copy is the words and never the faces they were drawn in.";
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of([Entry::Turn { turn: 1 }, answer(1, struck)]),
    );

    let taken = copied(&mut view);

    assert_eq!(
        taken, struck,
        "the copy is the answer as the model wrote it, so the tildes a struck run is written \
         with travel with it and no crossing is spelled out in their place"
    );
}

#[test]
fn ctrl_y_on_a_frame_with_nothing_marked_and_no_answer_hands_out_nothing() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );

    // Painted first, because a copy reads the span the last paint drew.
    frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let outcome = view.key(Key::Copy);

    assert_eq!(
        outcome,
        Outcome::Unchanged,
        "a copy of nothing is not a frame that has something to hand the terminal"
    );
}

#[test]
fn shift_down_extends_the_mark_a_whole_row_at_a_time() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(transcript()),
    );
    view.drag(Cell::new(1, 2), Cell::new(1, 60));
    let one_row = copied(&mut view);

    assert_eq!(
        view.key(Key::ShiftDown),
        Outcome::Edited,
        "Shift+↓ grows the mark, so there is a frame to paint"
    );
    let two_rows = copied(&mut view);
    assert_eq!(
        two_rows.lines().count(),
        one_row.lines().count() + 1,
        "one press is one whole row more: {one_row:?} became {two_rows:?}"
    );
    assert!(
        two_rows.starts_with(&one_row),
        "and it grew below the row it had: {two_rows:?}"
    );

    assert_eq!(
        view.key(Key::ShiftUp),
        Outcome::Edited,
        "Shift+↑ gives the row back"
    );
    assert_eq!(
        copied(&mut view),
        one_row,
        "and the mark is the one row it was"
    );
}

#[test]
fn the_shift_arrows_do_nothing_when_no_mark_has_been_dragged() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(transcript()),
    );
    frame::painted(&mut view, SCREEN.0, SCREEN.1);

    assert_eq!(
        view.key(Key::ShiftDown),
        Outcome::Unchanged,
        "there is no mark to grow, so nothing is painted and nothing is copied"
    );
    assert_eq!(view.mark(), None, "and no mark was invented from the key");
}

#[test]
fn a_heading_drawn_large_is_marked_in_the_mark_ink_and_copied_as_its_words() {
    let mut view = View::of(
        caps::kitty(),
        Facts::default().asked(),
        Transcript::of(vec![answer(1, "# A gate rule")]),
    );
    view.drag(Cell::new(0, 0), Cell::new(40, 100));

    let frame = frame::painted(&mut view, SCREEN.0, SCREEN.1);
    let heading = (0..SCREEN.1)
        .find(|y| frame.row(*y).contains("A   g a t e"))
        .expect("the heading is drawn as the cells the sizing leaves");

    for x in 0..SCREEN.0 {
        if frame.cell(x, heading).symbol().trim().is_empty() {
            continue;
        }
        assert_eq!(
            frame.ink(x, heading),
            Some(Ink::Mark),
            "column {x} of the heading is not the mark's, so the mark is drawn somewhere other \
             than the cells it covers: {:?}",
            frame.row(heading)
        );
    }
    assert!(
        copied(&mut view).contains("A gate rule"),
        "and the mark copies the heading as its words"
    );
}
