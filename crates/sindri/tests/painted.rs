//! What one paint pass hands back: the destinations the frame names, the runs it draws at more than
//! one cell, and the rows a press chooses, read off the value rather than off the frame.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::composer::{Key, Outcome};
use sindri::mark::Cell;
use sindri::pointer::Pointer;
use sindri::rows::Scroll;
use sindri::view::{Painted, View};
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

fn answer(text: &str) -> Vec<Entry> {
    vec![Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact(text),
    }]
}

fn painted_and_told(view: &mut View, cols: u16, rows: u16) -> (Buffer, Painted) {
    let area = Rect::new(0, 0, cols, rows);
    let mut buffer = Buffer::empty(area);
    let told = view.paint(area, &mut buffer);
    (buffer, told)
}

fn symbols(buffer: &Buffer, y: u16, x: u16, width: u16) -> Vec<String> {
    (x..x.saturating_add(width))
        .map(|x| {
            buffer
                .cell((x, y))
                .expect("a cell of the row")
                .symbol()
                .to_owned()
        })
        .collect()
}

fn drawn_at(buffer: &Buffer, said: &str) -> (u16, u16) {
    let wanted: Vec<String> = said.chars().map(|glyph| glyph.to_string()).collect();
    let area = buffer.area;
    for y in area.y..area.bottom() {
        let row = symbols(buffer, y, area.x, area.width);
        if let Some(at) = row
            .windows(wanted.len())
            .position(|held| held == wanted.as_slice())
        {
            return (
                y,
                area.x.saturating_add(u16::try_from(at).expect("a column")),
            );
        }
    }
    panic!("the frame draws {said:?} on no row of it");
}

#[test]
fn a_pass_hands_back_the_destination_a_row_named() {
    let mut view = View::of(
        answer("see [the gate](https://example.test/gate)"),
        Scroll::Tail,
    );
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let (row, column) = drawn_at(&buffer, "the gate");

    assert_eq!(
        told.links().at(row, column),
        Some("https://example.test/gate"),
        "the cells the row spells as a way to the gate name that destination once the pass has \
         been handed back"
    );
}

#[test]
fn a_pass_hands_back_the_run_it_draws_at_more_than_one_cell() {
    let mut view = View::of(answer("# A gate rule"), Scroll::Tail);
    view.set_sizing(true);
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let laid = told.sizes().laid();

    assert_eq!(laid.len(), 1, "one heading is one run: {laid:?}");
    let run = &laid[0];
    assert_eq!(
        run.run().words(),
        "A gate rule",
        "the run is the heading the row spells"
    );
    assert_eq!(
        symbols(&buffer, run.row(), run.left(), run.run().width()).concat(),
        run.run().spelling(),
        "and it was laid on the row it was drawn on, as the cells the sizing leaves"
    );
}

#[test]
fn a_press_on_a_row_the_pass_laid_chooses_that_row() {
    let mut view = View::boot();
    view.set_pointer(true);
    view.key(Key::Char('/'));
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let (row, _) = drawn_at(&buffer, "/quit");

    assert_eq!(
        view.pointer(&told, Pointer::Press(Cell::new(row, 40))),
        Outcome::Quit,
        "the row the press landed on is the row the pass laid `/quit` on"
    );
}

#[test]
fn a_pass_under_a_card_tells_the_terminal_nothing_of_the_frame_beneath_it() {
    let mut view = View::of(
        answer("# A gate rule\n\nsee [the gate](https://example.test/gate)"),
        Scroll::Tail,
    );
    view.set_sizing(true);
    view.set_pointer(true);
    view.key(Key::Char('/'));
    let (buffer, told) = painted_and_told(&mut view, 120, 40);

    assert!(
        told.links().is_empty(),
        "the frame under a card is no way to anywhere, so the pass names no destination"
    );
    assert!(
        told.sizes().laid().is_empty(),
        "and no heading of it is drawn large: the scrim is read as the words it holds"
    );
    let (row, _) = drawn_at(&buffer, "/quit");
    assert_eq!(
        view.pointer(&told, Pointer::Press(Cell::new(row, 40))),
        Outcome::Quit,
        "where the rows the card itself laid are still the rows a press chooses"
    );
}
