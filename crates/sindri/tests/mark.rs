//! The mark's contract with the clipboard, proved where a reader's eyes are: a drag on a headless buffer, the
//! frame painted from it, the cells the highlight covers read back out of that buffer, and those bytes held
//! to what `Ctrl-Y` hands out. A snapshot would compare the frame with a copy of itself.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

use sindri::composer::{Key, Outcome};
use sindri::ink::Ink;
use sindri::mark::Cell;
use sindri::palette::Palette;
use sindri::rows::Scroll;
use sindri::view::View;
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

fn painted(view: &mut View) -> Buffer {
    let (cols, rows) = SCREEN;
    let mut terminal =
        Terminal::new(TestBackend::new(cols, rows)).expect("a headless screen of that size");
    let completed = terminal
        .draw(|frame| view.paint(frame.area(), frame.buffer_mut()))
        .expect("the frame is painted");
    completed.buffer.clone()
}

fn highlighted(buffer: &Buffer, palette: &Palette) -> String {
    let area = buffer.area;
    let mut rows: Vec<String> = Vec::new();
    for y in area.y..area.bottom() {
        let mut line = String::new();
        for x in area.x..area.right() {
            let cell = buffer
                .cell((x, y))
                .unwrap_or_else(|| panic!("row {y}, column {x} is outside the frame it is in"));
            if Ink::of(cell, palette) != Some(Ink::Mark) {
                // The mark covers one run of cells a row, so the first cell of it that is not marked
                // ends the run this row carries.
                if !line.is_empty() {
                    break;
                }
                continue;
            }
            line.push_str(cell.symbol());
        }
        if !line.is_empty() {
            rows.push(line);
        }
    }
    rows.join("\n")
}

/// What `Ctrl-Y` hands the terminal, read after the frame the key changed has been painted.
fn copied(view: &mut View) -> String {
    painted(view);
    match view.key(Key::Copy) {
        Outcome::Copy(words) => words,
        other => panic!("Ctrl-Y is a copy, and this frame answered it {other:?}"),
    }
}

#[test]
fn what_the_highlight_covers_is_byte_for_byte_what_the_copy_takes() {
    let mut view = View::of(transcript(), Scroll::Tail);
    view.drag(Cell::new(1, 2), Cell::new(6, 80));

    let frame = painted(&mut view);
    let marked = highlighted(&frame, &view.palette());
    let taken = copied(&mut view);

    assert!(
        !taken.is_empty(),
        "the drag covered words, or there is nothing to hold the copy to: {frame:?}"
    );
    assert_eq!(
        marked, taken,
        "the cells the highlight covers are the bytes the copy hands out, which is §12's \
         \"a drag marks exactly the cells the copy will take\" as a comparison rather than \
         a promise"
    );
}

#[test]
fn a_redraw_keeps_the_highlight_and_ctrl_y_copies_it_again() {
    let mut view = View::of(transcript(), Scroll::Tail);
    view.drag(Cell::new(1, 2), Cell::new(4, 60));

    let before = painted(&mut view);
    let taken = copied(&mut view);
    let after = painted(&mut view);
    let again = copied(&mut view);

    assert!(!taken.is_empty(), "there was a mark to copy: {before:?}");
    assert_eq!(
        highlighted(&after, &view.palette()),
        highlighted(&before, &view.palette()),
        "a redraw keeps the highlight, because the mark is corners of the band and not a \
         picture of a frame that has been thrown away"
    );
    assert_eq!(
        again, taken,
        "and `Ctrl-Y` copies again with no fresh selection: the mark outlives the paint \
         that drew it (§12)"
    );
}

#[test]
fn ctrl_y_with_no_mark_copies_the_last_answer() {
    let mut view = View::of(transcript(), Scroll::Tail);

    let taken = copied(&mut view);

    assert_eq!(
        taken, NEWEST,
        "with nothing marked, `Ctrl-Y` copies the newest answer whole — the line breaks \
         the model wrote, and not the ones the band wrapped it into (§12)"
    );
}

#[test]
fn ctrl_y_on_a_frame_with_nothing_marked_and_no_answer_hands_out_nothing() {
    let mut view = View::boot();

    // Painted first, because a copy reads the span the last paint drew.
    painted(&mut view);
    let outcome = view.key(Key::Copy);

    assert_eq!(
        outcome,
        Outcome::Unchanged,
        "a copy of nothing is not a frame that has something to hand the terminal"
    );
}

#[test]
fn shift_down_extends_the_mark_a_whole_row_at_a_time() {
    let mut view = View::of(transcript(), Scroll::Tail);
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
    let mut view = View::of(transcript(), Scroll::Tail);
    painted(&mut view);

    assert_eq!(
        view.key(Key::ShiftDown),
        Outcome::Unchanged,
        "there is no mark to grow, so nothing is painted and nothing is copied"
    );
    assert_eq!(view.mark(), None, "and no mark was invented from the key");
}

#[test]
fn a_heading_drawn_large_is_marked_in_the_mark_ink_and_copied_as_its_words() {
    let mut view = View::of(vec![answer(1, "# A gate rule")], Scroll::Tail);
    view.set_sizing(true);
    view.drag(Cell::new(0, 0), Cell::new(40, 100));

    let frame = painted(&mut view);
    let palette = view.palette();
    let row_of = |y: u16| -> String {
        (frame.area.x..frame.area.right())
            .map(|x| buffer_cell(&frame, x, y).symbol().to_owned())
            .collect()
    };
    let heading = (frame.area.y..frame.area.bottom())
        .find(|y| row_of(*y).contains("A   g a t e"))
        .expect("the heading is drawn as the cells the sizing leaves");

    for x in frame.area.x..frame.area.right() {
        let cell = buffer_cell(&frame, x, heading);
        if cell.symbol().trim().is_empty() {
            continue;
        }
        assert_eq!(
            Ink::of(cell, &palette),
            Some(Ink::Mark),
            "column {x} of the heading is not the mark's, so the mark is drawn somewhere other \
             than the cells it covers: {:?}",
            row_of(heading)
        );
    }
    assert!(
        copied(&mut view).contains("A gate rule"),
        "and the mark copies the heading as its words"
    );
}

fn buffer_cell(frame: &Buffer, x: u16, y: u16) -> &ratatui::buffer::Cell {
    frame
        .cell((x, y))
        .unwrap_or_else(|| panic!("row {y}, column {x} is outside the frame it is in"))
}
