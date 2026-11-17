//! What one paint pass hands back: the frame it laid out, the destinations the frame names, the runs
//! it draws at more than one cell, the rows a press chooses, and what it drew of the transcript, read
//! off the value rather than off the frame.

mod caps; // the terminals these frames are drawn on

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::ledger;
use sindri::mark::Cell;
use sindri::palette::Palette;
use sindri::pointer::Pointer;
use sindri::rows::Scroll;
use sindri::view::{Painted, Transcript, View};
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
        Facts::default().asked(),
        Transcript::of(answer("see [the gate](https://example.test/gate)")),
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
    let mut view = View::of(
        caps::kitty(),
        Facts::default().asked(),
        Transcript::of(answer("# A gate rule")),
    );
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
    let mut view = View::of(
        caps::pointing(),
        Facts::default().asked(),
        Transcript::none(),
    );
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
        Facts::default().asked(),
        Transcript::of(answer(
            "# A gate rule\n\nsee [the gate](https://example.test/gate)",
        )),
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
        Facts::default().asked(),
        Transcript::of(answer("- gate\n- rule\n- mark")),
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
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(turns(200)),
    );
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
        Facts::default().asked(),
        Transcript::of([turns(1), answer("- gate\n- rule\n- mark")].concat()),
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
        Facts::default().asked(),
        Transcript::of(answer("- gate\n- rule\n- mark")),
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
        Facts::default().asked(),
        Transcript::of(answer("- gate\n- rule\n- mark")),
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

#[test]
fn a_pass_hands_back_the_bands_it_shared_the_frame_out_in() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(turns(3)),
    );
    let area = Rect::new(0, 0, 120, 40);
    let (_, told) = painted_and_told(&mut view, 120, 40);
    let laid = told
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");

    for band in [
        laid.transcript,
        laid.queue,
        laid.notice,
        laid.composer,
        laid.footer,
    ] {
        assert_eq!(
            (band.x, band.width),
            (area.x, area.width),
            "every band spans the frame's columns: {band:?}"
        );
    }
    assert_eq!(
        laid.transcript.y, area.y,
        "the transcript is given the frame's first row: {laid:?}"
    );
    assert_eq!(
        laid.transcript.bottom(),
        laid.notice.y,
        "the notice's band begins on the row the transcript's ends: {laid:?}"
    );
    assert_eq!(
        laid.notice.bottom(),
        laid.composer.y,
        "the composer's band begins on the row the notice's ends: {laid:?}"
    );
    assert_eq!(
        laid.queue.height, 0,
        "nothing queued is no queue row, so the queue takes nothing from the frame: {laid:?}"
    );
    assert_eq!(
        laid.composer.bottom(),
        laid.queue.y,
        "and its band is the composer's own rows: {laid:?}"
    );
    assert_eq!(
        laid.queue.bottom(),
        laid.footer.y,
        "the footer's band begins on the row the composer's ends: {laid:?}"
    );
    assert_eq!(
        (laid.footer.y, laid.footer.height),
        (area.bottom() - 1, 1),
        "the footer is the frame's last row and no other: {laid:?}"
    );
    assert_eq!(
        laid.card, None,
        "a frame with no screen and no commands card draws no card"
    );
}

/// The row a border glyph of a card is drawn on, and the columns it is drawn in: the glyph's first
/// and last cell along that row, read off the frame rather than measured from the card's width.
fn border(buffer: &Buffer, glyph: char) -> (u16, u16, u16) {
    let area = buffer.area;
    let wanted = glyph.to_string();
    for y in area.y..area.bottom() {
        let found: Vec<u16> = symbols(buffer, y, area.x, area.width)
            .iter()
            .enumerate()
            .filter(|(_, held)| **held == wanted)
            .map(|(at, _)| area.x + u16::try_from(at).expect("a column"))
            .collect();
        if let (Some(first), Some(last)) = (found.first(), found.last()) {
            return (y, *first, *last);
        }
    }
    panic!("the frame draws {glyph:?} on no row of it");
}

#[test]
fn a_pass_under_a_card_hands_back_the_body_the_screens_rows_were_drawn_in() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(turns(3)),
    );
    view.key(Key::Char('?'));
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let laid = told
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    let card = laid
        .card
        .expect("a frame with a screen open draws a card, and says where its body went");

    let (top, left, _) = border(&buffer, '\u{256d}');
    let (bottom, _, _) = border(&buffer, '\u{2570}');
    let (_, _, rule) = border(&buffer, '\u{2502}');

    assert_eq!(
        card.y,
        top + 1,
        "the screen's own rows begin on the row under the card's top border: {card:?}"
    );
    assert_eq!(
        card.bottom(),
        bottom,
        "and end on the row the key that leaves is drawn in: {card:?}"
    );
    let under = symbols(&buffer, top + 1, left + 1, rule.saturating_sub(left + 1));
    let begins = under
        .iter()
        .position(|held| held.trim() != "")
        .expect("the screen draws a row under the card's top border");
    assert_eq!(
        left + 1 + u16::try_from(begins).expect("a column"),
        card.x,
        "the screen's rows are set from the body's first column, past the card's border: {card:?}"
    );
    assert_eq!(
        card.right() + 1,
        rule,
        "and the body's last column is the one before the rule that closes it: {card:?}"
    );
}

/// Enough facts that the rail is drawn: a frame beside a rail has its rows cut before they are
/// shared out.
fn railed() -> Facts {
    Facts::default().asked().railed(ledger::Facts {
        wire: Some(String::from("anthropic")),
        model: Some(String::from("claude-sonnet-4-6")),
        ..ledger::Facts::default()
    })
}

#[test]
fn a_pass_beside_a_rail_hands_back_the_columns_the_transcript_was_given() {
    let mut view = View::of(
        caps::inked(Palette::TRUECOLOR),
        railed(),
        Transcript::of(turns(3)),
    );
    let (buffer, told) = painted_and_told(&mut view, 120, 40);
    let laid = told
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");

    let well = buffer
        .cell((0, 0))
        .expect("a cell of the frame's first row")
        .bg;
    let rail = (1..120)
        .find(|x| buffer.cell((*x, 0)).is_some_and(|cell| cell.bg != well))
        .expect("the rail's ground stands beside the transcript's");

    assert_eq!(
        laid.transcript.right(),
        rail,
        "the transcript is given the columns up to the rail's and not the frame's: {laid:?}"
    );
    assert!(
        laid.transcript.width < 120,
        "a frame beside a rail hands the transcript less than its own width: {laid:?}"
    );
}

#[test]
fn a_pass_hands_back_the_rail_beside_the_transcript_and_none_while_no_fact_is_known() {
    let mut view = View::of(Capabilities::default(), railed(), Transcript::of(turns(3)));
    let area = Rect::new(0, 0, 120, 40);
    let (_, told) = painted_and_told(&mut view, 120, 40);
    let laid = told
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    let (shape, rail) = laid
        .rail
        .expect("a frame whose facts know one is drawn with a rail beside the transcript");
    let rail = rail.area;

    assert_eq!(
        shape,
        ledger::Rail::Ledger,
        "a frame 120 columns wide is handed the ledger's column rather than its strip"
    );
    assert_eq!(
        (rail.y, rail.height),
        (laid.transcript.y, laid.transcript.height),
        "the rail stands the rows the transcript was given: {laid:?}"
    );
    assert_eq!(
        rail.right(),
        area.right(),
        "its last column is the frame's last column: {laid:?}"
    );

    let mut plain = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::of(turns(3)),
    );
    let (_, told) = painted_and_told(&mut plain, 120, 40);
    let laid = told
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");

    assert_eq!(
        laid.rail, None,
        "a frame whose facts know nothing draws no rail, so the value names none: {laid:?}"
    );
}

#[test]
fn the_caret_is_read_in_the_band_the_pass_drew_and_not_in_one_counted_again() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("1\n2\n3\n4\n5");
    let (_, told) = painted_and_told(&mut view, 40, 12);

    assert_eq!(
        view.cursor(&told),
        Some((4, 9)),
        "a five-row draft takes seven of the frame's twelve rows, so its last row is row nine \
         and the caret waits one column past the gutter, inside the box"
    );

    for _ in 0.."1\n2\n3\n4\n5".len() {
        view.key(Key::Backspace);
    }

    assert_eq!(
        view.cursor(&told),
        Some((3, 5)),
        "the empty draft stands in the band the pass drew, which began at row four: a band \
         counted again for a draft of one row would answer row ten instead"
    );
}

/// Whether `cell` is one the band `band` holds: a caret is never a cell outside it.
fn holds_cell(band: Rect, (x, y): (u16, u16)) -> bool {
    (band.x..band.right()).contains(&x) && (band.y..band.bottom()).contains(&y)
}

#[test]
fn a_frame_with_a_card_and_one_without_answer_their_caret_from_the_layout_they_handed_back() {
    let draft = |text: &str| {
        let mut view = View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        );
        view.paste(text);
        view
    };

    let mut plain = draft("a draft of mine");
    let (_, told) = painted_and_told(&mut plain, 40, 12);
    let laid = told
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    assert_eq!(laid.card, None, "a draft alone draws no card: {laid:?}");
    let caret = plain
        .cursor(&told)
        .expect("a draft takes the keys, so its caret has a cell");
    assert!(
        holds_cell(laid.composer, caret),
        "a frame with no card puts the caret in the composer's band the pass handed back: \
         {caret:?} in {:?}",
        laid.composer
    );

    let mut slashed = draft("");
    slashed.key(Key::Char('/'));
    let (_, told) = painted_and_told(&mut slashed, 40, 12);
    let laid = told
        .layout()
        .expect("a card over the frame is no frame of its own: the bands are still laid out");
    assert!(
        laid.card.is_some(),
        "a draft beginning with a slash draws the commands card: {laid:?}"
    );
    let caret = slashed
        .cursor(&told)
        .expect("the draft behind the card still takes the keys, so a caret is placed");
    assert!(
        holds_cell(laid.composer, caret),
        "and it is a cell of the same band, read off the same value: {caret:?} in {:?}",
        laid.composer
    );

    let mut screen = draft("");
    screen.key(Key::Char('?'));
    let (_, told) = painted_and_told(&mut screen, 40, 12);
    let laid = told
        .layout()
        .expect("a screen is drawn over a frame that was laid out under it");
    assert!(laid.card.is_some(), "a screen is drawn on a card: {laid:?}");
    assert_eq!(
        screen.cursor(&told),
        None,
        "and a screen is the whole frame with no draft in it, so it asks for no caret at all"
    );

    assert_eq!(
        plain.cursor(&Painted::none()),
        None,
        "a value no pass has laid a frame out in answers no caret"
    );
}
