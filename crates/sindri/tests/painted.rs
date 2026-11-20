//! What one paint pass hands back: the frame it laid out, the destinations the frame names, the runs
//! it draws at more than one cell, the rows a press chooses, and what it drew of the transcript, read
//! off the value rather than off the frame.

mod caps; // the terminals these frames are drawn on
mod frame; // one paint-and-read for every test

use frame::Frame;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::ink::{Faces, Ink, Look};
use sindri::ledger;
use sindri::mark::Cell;
use sindri::palette::Palette;
use sindri::pen::{self, Clip};
use sindri::pointer::Pointer;
use sindri::rows::Scroll;
use sindri::view::{Painted, Transcript, View};
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

fn turns(count: u32) -> Vec<Entry> {
    (1..=count).map(thought).collect()
}

/// One row of the band that says which turn it is: a turn draws no row of its own, so a
/// transcript of one row to a turn says which one they stand on with a thought.
fn thought(turn: u32) -> Entry {
    Entry::Reasoning {
        turn,
        text: SecretSet::default().redact(format!("turn {turn}")),
    }
}

fn answer(text: &str) -> Vec<Entry> {
    vec![Entry::Assistant {
        turn: 1,
        text: SecretSet::default().redact(text),
    }]
}

/// The band row whose cells hold `said`, counted from the band's first row.
fn row_of(frame: &Frame, area: Rect, said: &str) -> u16 {
    for y in area.y..area.bottom() {
        if frame.row(y).contains(said) {
            return y.saturating_sub(area.y);
        }
    }
    panic!("the frame draws {said:?} on no row of the band it was given");
}

/// One fence line wider than the band it is drawn in, and a row of prose under it.
const FENCE: &str = "```rust\nlet a_fence_line_wider_than_the_band_it_is_drawn_in_is_kept_in_\
                     the_shape_the_model_wrote_it_in_and_no_band_reflows_one = 1;\n```\n\n\
                     the prose under the fence";

/// What the frame's last row says.
fn footer(frame: &Frame, laid: &sindri::view::Layout) -> String {
    frame
        .row(laid.footer.y)
        .chars()
        .skip(usize::from(laid.footer.x))
        .take(usize::from(laid.footer.width))
        .collect()
}

fn drawn_at(frame: &Frame, said: &str) -> (u16, u16) {
    let wanted: Vec<char> = said.chars().collect();
    let area = frame.cells().area;
    for y in area.y..area.bottom() {
        let row: Vec<char> = frame.row(y).chars().collect();
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
    let frame = frame::painted(&mut view, 120, 40);
    let (row, column) = drawn_at(&frame, "the gate");

    assert_eq!(
        frame.told().links().at(row, column),
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
    let frame = frame::painted(&mut view, 120, 40);
    let laid = frame.told().sizes().laid();

    assert_eq!(laid.len(), 1, "one heading is one run: {laid:?}");
    let run = &laid[0];
    assert_eq!(
        run.run().words(),
        "A gate rule",
        "the run is the heading the row spells"
    );
    assert_eq!(
        frame
            .row(run.row())
            .chars()
            .skip(usize::from(run.left()))
            .take(usize::from(run.run().width()))
            .collect::<String>(),
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
    let frame = frame::painted(&mut view, 120, 40);
    let (row, _) = drawn_at(&frame, "/quit");

    assert_eq!(
        view.pointer(frame.told(), Pointer::Press(Cell::new(row, 40))),
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
    let frame = frame::painted(&mut view, 120, 40);

    assert!(
        frame.told().links().is_empty(),
        "the frame under a card is no way to anywhere, so the pass names no destination"
    );
    assert!(
        frame.told().sizes().laid().is_empty(),
        "and no heading of it is drawn large: the scrim is read as the words it holds"
    );
    let (row, _) = drawn_at(&frame, "/quit");
    assert_eq!(
        view.pointer(frame.told(), Pointer::Press(Cell::new(row, 40))),
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
    let frame = frame::painted(&mut view, 120, 40);

    assert_eq!(
        frame.told().drawn().total(),
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
    let frame = frame::painted(&mut view, 120, 40);
    let band = u32::from(frame.told().drawn().band());

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

    frame::painted(&mut view, 120, 40);
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
    let frame = frame::painted(&mut view, 120, 40);
    let (row, _) = drawn_at(&frame, "gate");

    assert_eq!(
        frame.told().drawn().entry(row),
        Some(1),
        "the row the answer's words are drawn on is the second entry's"
    );
    assert_eq!(
        frame.told().drawn().entry(row.saturating_sub(1)),
        Some(0),
        "and the row above it is the one the entry before was laid in"
    );
    assert_eq!(
        frame
            .told()
            .drawn()
            .entry(frame.told().drawn().band().saturating_sub(1)),
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
    let frame = frame::painted(&mut view, 120, 40);
    let (row, column) = drawn_at(&frame, "rule");

    view.drag(Cell::new(row, column), Cell::new(row, column + 3));
    let frame = frame::painted(&mut view, 120, 40);

    assert_eq!(
        frame.told().drawn().span().text(),
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
    let frame = frame::painted(&mut view, 120, 40);
    let (row, column) = drawn_at(&frame, "rule");
    view.drag(Cell::new(row, column), Cell::new(row, column + 3));
    frame::painted(&mut view, 120, 40);
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
    let frame = frame::painted(&mut view, 120, 40);
    let laid = frame
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
fn border(frame: &Frame, glyph: char) -> (u16, u16, u16) {
    let area = frame.cells().area;
    for y in area.y..area.bottom() {
        let found: Vec<u16> = frame
            .row(y)
            .chars()
            .enumerate()
            .filter(|(_, held)| *held == glyph)
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
        Transcript::of(answer("- gate\n- rule\n- mark")),
    );
    view.key(Key::Char('?'));
    let frame = frame::painted(&mut view, 120, 40);
    let laid = frame
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    let card = laid
        .card
        .expect("a frame with a screen open draws a card, and says where its body went");

    let (top, left, _) = border(&frame, '\u{256d}');
    let (bottom, _, _) = border(&frame, '\u{2570}');
    let (_, _, rule) = border(&frame, '\u{2502}');

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
    let under: Vec<char> = frame
        .row(top + 1)
        .chars()
        .skip(usize::from(left + 1))
        .take(usize::from(rule.saturating_sub(left + 1)))
        .collect();
    let begins = under
        .iter()
        .position(|held| !held.is_whitespace())
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
    let frame = frame::painted(&mut view, 120, 40);
    let laid = frame
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");

    let well = frame.cell(0, 0).bg;
    let rail = (1..120)
        .find(|x| frame.cell(*x, 0).bg != well)
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
    let frame = frame::painted(&mut view, 120, 40);
    let laid = frame
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
    let frame = frame::painted(&mut plain, 120, 40);
    let laid = frame
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");

    assert_eq!(
        laid.rail, None,
        "a frame whose facts know nothing draws no rail, so the value names none: {laid:?}"
    );
}

#[test]
fn the_caret_is_placed_in_the_band_the_pass_drew_and_not_in_one_counted_again() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("1\n2\n3\n4\n5");
    let frame = frame::painted(&mut view, 40, 12);

    assert_eq!(
        frame.told().caret(),
        Some(Position::new(4, 8)),
        "a five-row draft takes nine of the frame's twelve rows, so its last row is row eight \
         and the caret is placed one column past the gutter, inside the box"
    );

    for _ in 0.."1\n2\n3\n4\n5".len() {
        view.key(Key::Backspace);
    }

    let frame = frame::painted(&mut view, 40, 12);
    let band = frame
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out")
        .composer;

    assert_eq!(
        frame.told().caret(),
        Some(Position::new(band.x + 3, band.y + 1)),
        "the empty draft's caret is placed in the band this pass drew, on its first draft row \
         and on the first glyph cell of the placeholder"
    );
}

#[test]
fn a_frame_with_no_card_keeps_the_caret_placed_in_the_band_the_pass_handed_back() {
    let mut plain = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    plain.paste("a draft of mine");
    let frame = frame::painted(&mut plain, 40, 12);
    let laid = frame
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    assert_eq!(laid.card, None, "a draft alone draws no card: {laid:?}");
    let caret = frame
        .caret()
        .expect("a draft takes the keys, so the pass places the caret in it");
    assert!(
        laid.composer.contains(caret),
        "a frame with no card places the caret in the composer's band the pass handed back: \
         {caret:?} in {:?}",
        laid.composer
    );

    let mut screen = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    screen.key(Key::Char('?'));
    let frame = frame::painted(&mut screen, 40, 12);
    let laid = frame
        .layout()
        .expect("a screen is drawn over a frame that was laid out under it");
    assert!(laid.card.is_some(), "a screen is drawn on a card: {laid:?}");
    assert_eq!(
        frame.told().caret(),
        None,
        "and a screen is the whole frame with no draft in it, so no caret is placed there"
    );
}

/// Every cell of `area` the frame draws in the mark ink, read off the cells a reader is shown.
fn mark_cells(frame: &Frame, area: Rect) -> Vec<(u16, u16)> {
    let mut found = Vec::new();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if Look::of(frame.cell(x, y), frame.palette()) == Some(Look::plain(Ink::Mark)) {
                found.push((x, y));
            }
        }
    }
    found
}

#[test]
fn a_typed_drafts_band_carries_no_cell_drawn_in_the_mark() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("a draft of mine");
    let frame = frame::painted(&mut view, 40, 12);
    let laid = frame
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");

    assert_eq!(
        mark_cells(&frame, laid.composer),
        Vec::new(),
        "the caret a reader sees is the terminal's own cursor, so no cell of the band a typed \
         draft stands in is stamped for it"
    );
}

#[test]
fn the_pass_places_the_cursor_one_column_past_the_last_glyph_of_the_last_row() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("1\n2\n3\n4\n5");
    let frame = frame::painted(&mut view, 40, 12);

    assert_eq!(
        frame.told().caret(),
        Some(Position::new(4, 8)),
        "the fifth row of a five-row draft is row eight of the frame, and its one glyph takes \
         column three, so the cursor waits on column four"
    );
}

#[test]
fn a_pass_counts_a_wide_glyph_as_two_columns_of_caret() {
    let caret_of = |draft: &str| {
        let mut view = View::of(
            Capabilities::default(),
            Facts::default().asked(),
            Transcript::none(),
        );
        view.paste(draft);
        frame::painted(&mut view, 40, 12)
            .caret()
            .expect("a draft takes the keys, so the pass places the cursor in it")
    };

    assert_eq!(
        caret_of("日本").x - caret_of("ab").x,
        2,
        "two wide glyphs take four columns where two narrow ones take two, so the caret waits \
         two columns further past them"
    );
}

#[test]
fn a_pass_where_a_reader_types_no_draft_hands_back_no_caret() {
    let mut commands = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    commands.key(Key::Char('/'));
    let frame = frame::painted(&mut commands, 60, 13);
    let laid = frame
        .layout()
        .expect("a pass under a card still laid the frame the card stands over");
    assert!(laid.card.is_some(), "the commands card is drawn: {laid:?}");
    assert_eq!(
        frame.told().caret(),
        None,
        "a cell a card holds is no cell the terminal's cursor is placed in"
    );

    let mut screen = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    screen.key(Key::Char('?'));
    let frame = frame::painted(&mut screen, 120, 40);
    assert_eq!(
        frame.told().caret(),
        None,
        "and neither does a screen held over the band, because no key reaches that draft"
    );
}

#[test]
fn a_card_that_leaves_the_carets_cell_leaves_the_cursor_in_that_same_cell() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.key(Key::Char('/'));
    let frame = frame::painted(&mut view, 120, 40);
    let laid = frame
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    let card = laid
        .card
        .expect("the commands card is drawn, and the pass says where its body went");
    let cursor = frame
        .caret()
        .expect("a card that never reaches the caret's cell leaves the cursor where it was");

    assert_eq!(
        cursor,
        Position::new(laid.composer.x + 4, laid.composer.y + 1),
        "the slash typed is one glyph on the draft's first row, so the cursor waits one column \
         past it"
    );
    assert!(
        !card.contains(cursor),
        "and the card leaves that cell to the band: {card:?}"
    );
    assert_eq!(
        mark_cells(&frame, laid.composer),
        Vec::new(),
        "so the cursor is the whole of the caret, and no cell of the band is drawn for it"
    );
}

#[test]
fn a_marked_fence_answers_the_sideways_key_and_the_footer_names_it_only_there() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(answer(FENCE)),
    );
    let before = frame::painted(&mut view, 100, 24);
    let laid = before
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = row_of(&before, laid.transcript, "a_fence_line");
    assert!(
        !footer(&before, &laid).contains("Shift+"),
        "a frame with nothing marked names no sideways key: {}",
        footer(&before, &laid)
    );

    view.drag(Cell::new(row, 0), Cell::new(row, 60));
    let marked = frame::painted(&mut view, 100, 24);
    let laid = marked
        .layout()
        .expect("a pass that drew a frame laid one out");
    assert!(
        footer(&marked, &laid).contains("Shift+→"),
        "the footer names the key that walks the fence the mark stands on: {}",
        footer(&marked, &laid)
    );

    assert_eq!(
        view.key(Key::ShiftRight),
        Outcome::Edited,
        "the marked fence answers the sideways key"
    );
    let walked = frame::painted(&mut view, 100, 24);
    assert_eq!(
        walked
            .row(laid.transcript.y + row)
            .chars()
            .skip(2)
            .take(60)
            .collect::<String>(),
        marked
            .row(laid.transcript.y + row)
            .chars()
            .skip(10)
            .take(60)
            .collect::<String>(),
        "the fence's line carries its words eight columns back, and its bar stays where it was"
    );
    for y in laid.transcript.y..laid.transcript.bottom() {
        if y == laid.transcript.y + row {
            continue;
        }
        assert_eq!(
            walked.row(y),
            marked.row(y),
            "no other row of the band moved with it"
        );
    }
}

#[test]
fn a_mark_that_covers_no_fence_line_answers_no_sideways_key() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(answer(FENCE)),
    );
    let before = frame::painted(&mut view, 100, 24);
    let laid = before
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = row_of(&before, laid.transcript, "the prose under the fence");

    view.drag(Cell::new(row, 0), Cell::new(row, 40));
    let marked = frame::painted(&mut view, 100, 24);
    let laid = marked
        .layout()
        .expect("a pass that drew a frame laid one out");

    assert!(
        !footer(&marked, &laid).contains("Shift+"),
        "the footer names no key the frame cannot honour: {}",
        footer(&marked, &laid)
    );
    assert_eq!(
        view.key(Key::ShiftRight),
        Outcome::Unchanged,
        "and a mark over prose walks nothing"
    );
}

#[test]
fn a_mark_that_opens_on_the_prose_and_runs_into_the_fence_still_walks_it() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(answer(FENCE)),
    );
    let before = frame::painted(&mut view, 100, 24);
    let laid = before
        .layout()
        .expect("a pass that drew a frame laid one out");
    let fence = row_of(&before, laid.transcript, "a_fence_line");
    let prose = row_of(&before, laid.transcript, "the prose under the fence");

    view.drag(Cell::new(prose, 0), Cell::new(fence, 20));
    let marked = frame::painted(&mut view, 100, 24);
    let laid = marked
        .layout()
        .expect("a pass that drew a frame laid one out");

    assert!(
        footer(&marked, &laid).contains("Shift+→"),
        "a drag the fence is part of is a drag over the fence, however far it runs past it: {}",
        footer(&marked, &laid)
    );
    assert_eq!(
        view.key(Key::ShiftRight),
        Outcome::Edited,
        "and the fence the drag covers answers the sideways key"
    );
}

#[test]
fn a_marked_fence_the_band_holds_whole_names_no_sideways_key() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(answer("```rust\nlet short = 1;\n```")),
    );
    let before = frame::painted(&mut view, 100, 24);
    let laid = before
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = row_of(&before, laid.transcript, "let short");

    view.drag(Cell::new(row, 0), Cell::new(row, 40));
    let marked = frame::painted(&mut view, 100, 24);
    let laid = marked
        .layout()
        .expect("a pass that drew a frame laid one out");

    assert!(
        !footer(&marked, &laid).contains("Shift+"),
        "a fence the band holds whole has no words past its edge, so the footer names no key that \
         would walk it: {}",
        footer(&marked, &laid)
    );
    assert_eq!(
        view.key(Key::ShiftRight),
        Outcome::Unchanged,
        "and the key it names nowhere answers nothing"
    );
}

#[test]
fn a_marked_line_of_a_cut_fence_answers_the_key_however_short_the_line_is() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(answer(
            "```rust\nlet a_fence_line_wider_than_the_band_it_is_drawn_in_is_kept_in_the_shape_\
             the_model_wrote_it_in = 1;\nlet short = 2;\n```",
        )),
    );
    let before = frame::painted(&mut view, 100, 24);
    let laid = before
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = row_of(&before, laid.transcript, "let short");

    view.drag(Cell::new(row, 0), Cell::new(row, 40));
    frame::painted(&mut view, 100, 24);
    assert_eq!(
        view.key(Key::ShiftRight),
        Outcome::Edited,
        "the mark stands on one line of a fence the band cut, and the offset covers every row of \
         that fence"
    );
}

#[test]
fn a_width_change_drops_the_walk_a_fence_was_left_in() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(answer(FENCE)),
    );
    let wide = frame::painted(&mut view, 120, 24);
    let laid = wide
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = row_of(&wide, laid.transcript, "a_fence_line");

    view.drag(Cell::new(row, 0), Cell::new(row, 119));
    frame::painted(&mut view, 120, 24);
    assert_eq!(view.key(Key::ShiftRight), Outcome::Edited);
    let nudged = frame::painted(&mut view, 120, 24);
    let laid = nudged
        .layout()
        .expect("a pass that drew a frame laid one out");
    let walked = nudged.row(laid.transcript.y + row);
    assert!(
        !walked.contains("let a_fence_line"),
        "the fence was left walked, so its line opens eight columns in: {walked:?}"
    );

    let narrow = frame::painted(&mut view, 60, 24);
    let narrow: String = narrow.rows().concat();
    assert!(
        narrow.contains("let a_fence_line"),
        "a row laid again forgets the walk, which is what a width change is: {narrow:?}"
    );
}

#[test]
fn one_pass_hands_back_the_cells_the_rows_the_words_and_the_caret() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::none(),
    );
    view.paste("a draft");

    let frame = frame::painted(&mut view, 120, 40);

    assert_eq!(
        frame.cells().area,
        Rect::new(0, 0, 120, 40),
        "the cells the pass left cover the screen"
    );
    assert_eq!(frame.rows().len(), 40, "one row to a row of the screen");

    let at = frame
        .caret()
        .expect("the pass places its caret in the draft it drew");
    let laid = frame
        .layout()
        .expect("a pass that drew a frame laid it out");
    assert!(
        laid.composer.contains(at),
        "the caret waits in the band the draft is typed on: {at:?} is outside {:?}",
        laid.composer
    );

    let row = frame.row(at.y);
    assert_eq!(row.chars().count(), 120, "a row is full width");
    assert!(
        row.contains("a draft"),
        "the row the draft is typed on says it: {row:?}"
    );
    assert_eq!(
        frame.words()[usize::from(at.y - frame.cells().area.y)],
        row.trim_end(),
        "a word is the row with the trailing blanks cut off"
    );

    let typed: String = row.chars().take(usize::from(at.x)).collect();
    assert!(
        typed.ends_with("a draft"),
        "the caret waits in the cell after the words typed: {typed:?}"
    );
}

#[test]
fn the_text_a_golden_holds_names_every_cell_and_the_look_it_wears() {
    let palette = Palette::TRUECOLOR;
    let emphasis = Look::new(Ink::Accent, Faces::BOLD.and(Faces::ITALIC));
    let linked = Look::new(Ink::Muted, Faces::ITALIC).underlined(Ink::Dim);
    let area = Rect::new(0, 0, 2, 1);
    let mut cells = Buffer::empty(area);
    pen::write(
        &mut cells,
        area,
        &[("a", emphasis), ("b", linked)],
        Clip::Whole,
        &palette,
    );

    let drawn = frame::holding(cells, palette, Painted::none()).text("two-faces");
    let written = drawn.lines().nth(1).expect("the one row of the frame");
    assert_eq!(
        written,
        format!(
            "00 |ab| {}:1 {}:1",
            "accent+bold+italic", "muted+italic~dim"
        ),
        "the faces a cell wears and the line under it are written in one order, so one frame has \
         one picture of it"
    );
}

#[test]
#[should_panic(expected = "is not one glyph")]
fn a_cell_holding_two_glyphs_is_no_frame() {
    let palette = Palette::TRUECOLOR;
    let area = Rect::new(0, 0, 1, 1);
    let mut cells = Buffer::empty(area);
    cells
        .cell_mut((0, 0))
        .expect("the one cell of the frame")
        .set_symbol("ab");

    frame::holding(cells, palette, Painted::none()).rows();
}
