//! What one paint pass hands back: the destinations the frame names, the runs it draws at more than
//! one cell, the rows a press chooses, and what it drew of the transcript, read off the value rather
//! than off the frame.

mod caps; // the terminals these frames are drawn on

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::mark::Cell;
use sindri::pointer::Pointer;
use sindri::rows::Scroll;
use sindri::view::{Painted, View};
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

fn turns(count: u32) -> Vec<Entry> {
    (1..=count).map(|turn| Entry::Turn { turn }).collect()
}

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
        Capabilities::default(),
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
    let mut view = View::of(caps::kitty(), answer("# A gate rule"), Scroll::Tail);
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
    let mut view = View::boot(caps::pointing());
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
        caps::kitty(),
        answer("# A gate rule\n\nsee [the gate](https://example.test/gate)"),
        Scroll::Tail,
    );
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

#[test]
fn a_pass_hands_back_the_height_the_whole_transcript_held() {
    let mut view = View::of(
        Capabilities::default(),
        answer("- gate\n- rule\n- mark"),
        Scroll::Tail,
    );
    let (_, told) = painted_and_told(&mut view, 120, 40);

    assert_eq!(
        told.drawn().total(),
        Some(3),
        "an answer of three short lines is a transcript three rows tall"
    );
}

#[test]
fn a_pass_hands_back_the_band_a_page_moves_by() {
    let mut view = View::of(Capabilities::default(), turns(200), Scroll::Tail);
    let (_, told) = painted_and_told(&mut view, 120, 40);
    let band = u32::from(told.drawn().band());

    assert_eq!(
        view.key(Key::PageUp),
        Outcome::Edited,
        "a transcript deeper than its band is a page back"
    );
    assert_eq!(
        view.scroll(),
        Scroll::Back(band),
        "one page is the band the pass recorded"
    );

    painted_and_told(&mut view, 120, 40);
    assert_eq!(view.key(Key::PageUp), Outcome::Edited, "a second page");
    assert_eq!(
        view.scroll(),
        Scroll::Back(2 * band),
        "the row the second pass gives the notice is counted, so two pages are two bands"
    );
}

#[test]
fn a_pass_hands_back_the_entry_it_laid_on_a_row() {
    let mut view = View::of(
        Capabilities::default(),
        [turns(1), answer("- gate\n- rule\n- mark")].concat(),
        Scroll::Tail,
    );
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let (row, _) = drawn_at(&buffer, "gate");

    assert_eq!(
        told.drawn().entry(row),
        Some(1),
        "the row the answer's words are drawn on is the second entry's"
    );
    assert_eq!(
        told.drawn().entry(row.saturating_sub(1)),
        Some(0),
        "and the row above it is the one the entry before was laid in"
    );
    assert_eq!(
        told.drawn().entry(told.drawn().band().saturating_sub(1)),
        None,
        "a row of the band the pass laid nothing on answers no entry"
    );
}

#[test]
fn a_pass_hands_back_the_span_a_copy_takes() {
    let mut view = View::of(
        Capabilities::default(),
        answer("- gate\n- rule\n- mark"),
        Scroll::Tail,
    );
    let (buffer, _) = painted_and_told(&mut view, 120, 40);
    let (row, column) = drawn_at(&buffer, "rule");

    view.drag(Cell::new(row, column), Cell::new(row, column + 3));
    let (_, told) = painted_and_told(&mut view, 120, 40);

    assert_eq!(
        told.drawn().span().text(),
        "rule",
        "the cells the drag covers are the words `Ctrl-Y` hands out, read off the pass"
    );
}

#[test]
fn an_act_that_replaces_the_transcript_empties_what_the_pass_drew() {
    let mut view = View::of(
        Capabilities::default(),
        answer("- gate\n- rule\n- mark"),
        Scroll::Tail,
    );
    let (buffer, _) = painted_and_told(&mut view, 120, 40);
    let (row, column) = drawn_at(&buffer, "rule");
    view.drag(Cell::new(row, column), Cell::new(row, column + 3));
    painted_and_told(&mut view, 120, 40);
    assert_eq!(
        view.key(Key::Copy),
        Outcome::Copy(String::from("rule")),
        "the mark the last pass drew is the words a copy takes"
    );

    for glyph in ['/', 'n', 'e', 'w'] {
        view.key(Key::Char(glyph));
    }
    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "`/new` starts the conversation again"
    );
    assert_eq!(
        view.key(Key::Copy),
        Outcome::Unchanged,
        "a copy of a transcript that is gone hands out nothing, with no repaint in between"
    );
}
