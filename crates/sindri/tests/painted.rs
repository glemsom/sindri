mod caps;
mod frame;

use frame::Frame;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};

use sindri::capability::Capabilities;
use sindri::composer::{Key, Outcome};
use sindri::facts::Facts;
use sindri::ink::{Faces, Ink, Look};
use sindri::ledger;
use sindri::mark::Cell;
use sindri::palette::{Band, Palette};
use sindri::pen::{self, Clip};
use sindri::pointer::Pointer;
use sindri::rows::Scroll;
use sindri::view::{Transcript, View};
use sindri_core::records::Entry;
use sindri_core::redact::SecretSet;

fn turns(count: u32) -> Vec<Entry> {
    (1..=count).map(thought).collect()
}

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

const FENCE: &str = "```rust\nlet a_fence_line_wider_than_the_band_it_is_drawn_in_is_kept_in_\
                     the_shape_the_model_wrote_it_in_and_no_band_reflows_one = 1;\n```\n\n\
                     the prose under the fence";

fn footer(frame: &Frame, laid: &sindri::view::Layout) -> String {
    frame
        .row(laid.footer.y)
        .chars()
        .skip(usize::from(laid.footer.x))
        .take(usize::from(laid.footer.width))
        .collect()
}

#[test]
fn a_pass_hands_back_the_destination_a_row_named() {
    let mut view = View::of(
        Capabilities::default(),
        Facts::default().asked(),
        Transcript::of(answer("see [the gate](https://example.test/gate)")),
    );
    let frame = frame::painted(&mut view, 120, 40);
    let (row, column) = frame.drawn_at("the gate");

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
    let root = tempfile::tempdir().expect("a temp workspace");
    let mut view = View::of(
        caps::pointing(),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::none(),
    );
    view.key(Key::Char('/'));
    let frame = frame::painted(&mut view, 120, 40);
    let (row, _) = frame.drawn_at("/quit");

    assert_eq!(
        view.pointer(frame.told(), Pointer::Press(Cell::new(row, 40))),
        Outcome::Quit,
        "the row the press landed on is the row the pass laid `/quit` on"
    );
}

#[test]
fn a_pass_under_a_card_tells_the_terminal_nothing_of_the_frame_beneath_it() {
    let root = tempfile::tempdir().expect("a temp workspace");
    let mut view = View::of(
        caps::kitty(),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
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
    let (row, _) = frame.drawn_at("/quit");
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
    let (row, _) = frame.drawn_at("gate");

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
    let (row, column) = frame.drawn_at("rule");

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
    let root = tempfile::tempdir().expect("a temp workspace");
    let mut view = View::of(
        Capabilities::default(),
        Facts::default()
            .asked()
            .working(root.path().to_path_buf(), None),
        Transcript::of(answer("- gate\n- rule\n- mark")),
    );
    let frame = frame::painted(&mut view, 120, 40);
    let (row, column) = frame.drawn_at("rule");
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
        .told()
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
        laid.composer.bottom(),
        area.bottom(),
        "the composer's band takes the frame's last row, which the footer had: {laid:?}"
    );
    assert_eq!(
        (laid.footer.y, laid.footer.height),
        (laid.composer.bottom() - 1, 1),
        "and the hints are drawn on that band's last row: {laid:?}"
    );
    assert_eq!(
        laid.card, None,
        "a frame with no screen and no commands card draws no card"
    );
}

fn border(frame: &Frame, glyph: char) -> (u16, u16, u16) {
    let area = frame.area();
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
        .told()
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
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");

    let well = frame.cell(0, 0).bg;
    let band = laid.rail.expect("a frame this wide holds a ledger").1;

    assert_eq!(
        laid.transcript.right(),
        band.area.x,
        "the transcript is given the columns up to the rail's and not the frame's: {laid:?}"
    );
    for x in laid.transcript.x..laid.transcript.right() {
        assert_eq!(
            frame.cell(x, 0).bg,
            well,
            "column {x} of the transcript is not the well the band is read on: no column of the \
             columns the frame hands it is claimed by the rail beside it"
        );
    }
    assert_eq!(
        frame.cell(band.area.x, 0).bg,
        Palette::TRUECOLOR.edge_ink(),
        "the seam stands in the rail's own first column, and the transcript keeps every column \
         the rail leaves it: {laid:?}"
    );
    assert!(
        laid.transcript.width < 120,
        "a frame beside a rail hands the transcript less than its own width: {laid:?}"
    );
}

#[test]
fn the_rail_holds_the_frames_right_edge_above_the_band_and_no_fact_draws_none() {
    let mut view = View::of(Capabilities::default(), railed(), Transcript::of(turns(3)));
    let area = Rect::new(0, 0, 120, 40);
    let frame = frame::painted(&mut view, 120, 40);
    let laid = frame
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    let (shape, band) = laid
        .rail
        .expect("a frame whose facts know one is drawn with a rail beside the bands");
    let rail = band.area;

    assert_eq!(
        shape,
        ledger::Rail::Ledger,
        "a frame 120 columns wide is handed the ledger's column rather than its strip"
    );
    assert_eq!(
        (rail.y, rail.height),
        (area.y, laid.composer.y - area.y),
        "the rail's band is the frame's own height less the rows the composer holds: {laid:?}"
    );
    assert_eq!(
        (rail.right(), rail.bottom()),
        (area.right(), laid.composer.y),
        "so its last column is the frame's own and its last row is the one above the composer: \
         {laid:?}"
    );
    assert_eq!(
        laid.transcript.bottom(),
        rail.bottom(),
        "the transcript stops at the composer's head, as the rail does: {laid:?}"
    );

    let mut plain = View::of(
        Capabilities::default(),
        Facts::default(),
        Transcript::of(turns(3)),
    );
    let frame = frame::painted(&mut plain, 120, 40);
    let laid = frame
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");

    assert_eq!(
        laid.rail, None,
        "a frame whose facts know nothing draws no rail, so the value names none: {laid:?}"
    );
    assert_eq!(
        laid.transcript.width, 120,
        "and the bands keep every column of a frame that has no rail to stand beside: {laid:?}"
    );
}

#[test]
fn the_transcript_and_the_notice_stand_in_its_columns_and_the_bands_in_the_frames() {
    let palette = Palette::TRUECOLOR;
    let mut view = View::of(caps::inked(palette), railed(), Transcript::of(turns(3)));
    let area = Rect::new(0, 0, 120, 40);
    let frame = frame::painted(&mut view, 120, 40);
    let laid = frame
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    let rail = laid.rail.expect("a rail above the bands").1.area;

    assert_eq!(
        rail.x,
        laid.transcript.right(),
        "the rail's seam opens the column the transcript's band ends at: {laid:?}"
    );
    for band in [laid.transcript, laid.notice] {
        assert_eq!(
            (band.x, band.width),
            (area.x, rail.x),
            "the transcript's band stands in its own columns and not the frame's: {laid:?}"
        );
    }
    for band in [laid.composer, laid.footer, laid.queue] {
        assert_eq!(
            (band.x, band.width),
            (area.x, area.width),
            "a band the composer holds stands in the frame's columns, with no rail beside it: \
             {laid:?}"
        );
    }
    for y in rail.y..rail.bottom() {
        for x in rail.x..area.right() {
            let ground = frame.cell(x, y).bg;
            let wanted = match x == rail.x {
                true => palette.edge_ink(),
                false => palette.band(Band::Rail),
            };
            assert_eq!(
                ground, wanted,
                "row {y}, column {x} of the rail is {ground:?} where the rail's band is {wanted:?}, \
                 and the band is one tone down every row of it but the one column the frame's \
                 seam stands in"
            );
        }
    }
}

#[test]
fn below_the_ledgers_floor_the_strip_takes_the_transcripts_last_row_and_no_column() {
    let mut view = View::of(Capabilities::default(), railed(), Transcript::of(turns(3)));
    let area = Rect::new(0, 0, 80, 24);
    let frame = frame::painted(&mut view, 80, 24);
    let laid = frame
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    let (shape, band) = laid
        .rail
        .expect("a frame whose facts know one is drawn with a rail");

    assert_eq!(
        shape,
        ledger::Rail::Strip,
        "a frame 80 columns wide is handed the strip rather than the ledger's column"
    );
    assert_eq!(
        band.area,
        Rect {
            y: laid.transcript.bottom(),
            height: 1,
            ..area
        },
        "the strip is the row the transcript's band gave up, across the frame's own columns"
    );
    assert_eq!(
        (laid.composer.x, laid.composer.width),
        (area.x, area.width),
        "and it takes no column of the bands beside it: {laid:?}"
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
        Some(Position::new(3, 9)),
        "a five-row draft takes nine of the frame's twelve rows, so its last row is row nine \
         and the caret is placed one column past the gutter, inside the band"
    );

    for _ in 0.."1\n2\n3\n4\n5".len() {
        view.key(Key::Backspace);
    }

    let frame = frame::painted(&mut view, 40, 12);
    let band = frame
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out")
        .composer;

    assert_eq!(
        frame.told().caret(),
        Some(Position::new(band.x + 2, band.y + 1)),
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
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    assert_eq!(laid.card, None, "a draft alone draws no card: {laid:?}");
    let caret = frame
        .told()
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
        .told()
        .layout()
        .expect("a screen is drawn over a frame that was laid out under it");
    assert!(laid.card.is_some(), "a screen is drawn on a card: {laid:?}");
    assert_eq!(
        frame.told().caret(),
        None,
        "and a screen is the whole frame with no draft in it, so no caret is placed there"
    );
}

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
        .told()
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
        Some(Position::new(3, 9)),
        "the fifth row of a five-row draft is row nine of the frame, and its one glyph takes \
         column two, so the cursor waits on column three"
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
            .told()
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
    let frame = frame::painted(&mut commands, 60, 11);
    let laid = frame
        .told()
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
        .told()
        .layout()
        .expect("a pass that drew a frame hands back the frame it laid out");
    let card = laid
        .card
        .expect("the commands card is drawn, and the pass says where its body went");
    let cursor = frame
        .told()
        .caret()
        .expect("a card that never reaches the caret's cell leaves the cursor where it was");

    assert_eq!(
        cursor,
        Position::new(laid.composer.x + 3, laid.composer.y + 1),
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
        .told()
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = before.row_holding(laid.transcript, "a_fence_line");
    assert!(
        !footer(&before, &laid).contains("Shift+"),
        "a frame with nothing marked names no sideways key: {}",
        footer(&before, &laid)
    );

    view.drag(Cell::new(row, 0), Cell::new(row, 60));
    let marked = frame::painted(&mut view, 100, 24);
    let laid = marked
        .told()
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
        .told()
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = before.row_holding(laid.transcript, "the prose under the fence");

    view.drag(Cell::new(row, 0), Cell::new(row, 40));
    let marked = frame::painted(&mut view, 100, 24);
    let laid = marked
        .told()
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
        .told()
        .layout()
        .expect("a pass that drew a frame laid one out");
    let fence = before.row_holding(laid.transcript, "a_fence_line");
    let prose = before.row_holding(laid.transcript, "the prose under the fence");

    view.drag(Cell::new(prose, 0), Cell::new(fence, 20));
    let marked = frame::painted(&mut view, 100, 24);
    let laid = marked
        .told()
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
        .told()
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = before.row_holding(laid.transcript, "let short");

    view.drag(Cell::new(row, 0), Cell::new(row, 40));
    let marked = frame::painted(&mut view, 100, 24);
    let laid = marked
        .told()
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
        .told()
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = before.row_holding(laid.transcript, "let short");

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
        .told()
        .layout()
        .expect("a pass that drew a frame laid one out");
    let row = wide.row_holding(laid.transcript, "a_fence_line");

    view.drag(Cell::new(row, 0), Cell::new(row, 119));
    frame::painted(&mut view, 120, 24);
    assert_eq!(view.key(Key::ShiftRight), Outcome::Edited);
    let nudged = frame::painted(&mut view, 120, 24);
    let laid = nudged
        .told()
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
        frame.area(),
        Rect::new(0, 0, 120, 40),
        "the cells the pass left cover the screen"
    );
    assert_eq!(frame.rows().len(), 40, "one row to a row of the screen");

    let at = frame
        .told()
        .caret()
        .expect("the pass places its caret in the draft it drew");
    let laid = frame
        .told()
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
        frame.words()[usize::from(at.y - frame.area().y)],
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

    let drawn = frame::holding(cells, palette).text("two-faces");
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
    let row = frame::row_of(written).expect("the row a golden holds is the row it reads back");
    assert_eq!(
        row.looks,
        [emphasis, linked],
        "and the looks it reads back are the two the cells were written in"
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

    frame::holding(cells, palette).rows();
}

#[test]
fn a_rich_rail_sets_its_labels_inside_their_existing_cells() {
    let rail = ledger::Facts {
        model: Some("a-model".into()),
        ..Default::default()
    };
    let mut view = View::of(
        caps::kitty(),
        Facts::default().asked().railed(rail),
        Transcript::none(),
    );
    let frame = frame::painted(&mut view, 160, 40);
    assert!(
        frame
            .told()
            .sizes()
            .laid()
            .iter()
            .any(|laid| laid.run().words() == "model"),
        "the rail label carries its sizing"
    );
}

#[test]
fn a_rich_rail_gives_the_state_two_rows_without_moving_the_model() {
    let rail = ledger::Facts {
        state: Some(sindri_core::run::State::Running),
        model: Some("a-model".into()),
        ..Default::default()
    };
    let mut rich = View::of(
        caps::faced(false),
        Facts::default().asked().railed(rail.clone()),
        Transcript::none(),
    );
    let mut plain = View::of(
        Capabilities::default(),
        Facts::default().asked().railed(rail),
        Transcript::none(),
    );
    let rich = frame::painted(&mut rich, 160, 40);
    let plain = frame::painted(&mut plain, 160, 40);
    assert!(
        rich.told()
            .sizes()
            .laid()
            .iter()
            .any(|laid| laid.run().scale() == 2 && laid.run().words().contains("running"))
    );
    assert_eq!(rich.drawn_at("a-model"), plain.drawn_at("a-model"));
}

#[test]
fn a_rich_rail_sets_the_spend_figure_and_its_unit_without_losing_facts() {
    let rail = ledger::Facts {
        spend: sindri_core::records::Usage {
            input: Some(48_200),
            output: Some(3_100),
            total: Some(51_300),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut view = View::of(
        caps::faced(false),
        Facts::default().asked().railed(rail),
        Transcript::none(),
    );
    let frame = frame::painted(&mut view, 160, 40);
    let runs = frame.told().sizes().laid();
    assert!(
        runs.iter()
            .any(|laid| laid.run().scale() == 2 && laid.run().words() == "48.2")
    );
    assert!(
        runs.iter()
            .any(|laid| laid.run().words() == "k" && laid.run().fraction() == Some((1, 4, 1, 0)))
    );
    frame.drawn_at("51.3k");
    frame.drawn_at("3.1k");
}

#[test]
fn rail_sizing_keeps_plain_prose_and_unsized_terminals_unchanged() {
    let rail = ledger::Facts {
        model: Some("界-model".into()),
        state: Some(sindri_core::run::State::Running),
        ..Default::default()
    };
    for (caps, sized) in [
        (caps::faced(false), true),
        (caps::pictured().faced(false), false),
        (Capabilities::default(), false),
    ] {
        let mut view = View::of(
            caps,
            Facts::default().asked().railed(rail.clone()),
            Transcript::of(answer("ordinary prose")),
        );
        let frame = frame::painted(&mut view, 160, 40);
        let (row, column) = frame.drawn_at("ordinary prose");
        assert!(frame.told().sizes().covers(row, column).is_none());
        assert_eq!(!frame.told().sizes().laid().is_empty(), sized);
        frame.drawn_at("-model");
    }
}

#[test]
fn a_sized_rail_and_splash_keep_deliberately_reserved_cells() {
    let rail = ledger::Facts {
        model: Some("a-model".into()),
        state: Some(sindri_core::run::State::Running),
        ..Default::default()
    };
    for (facts, name) in [
        (Facts::default().asked(), "rail-sized-160x40"),
        (Facts::default(), "splash-sized-160x40"),
    ] {
        let mut view = View::of(
            caps::faced(false),
            facts.railed(rail.clone()),
            Transcript::none(),
        );
        let frame = frame::painted(&mut view, 160, 40);
        frame.snapshot(name, name);
    }
}

#[test]
fn a_state_that_cannot_fit_keeps_its_words_and_the_strip_keeps_one_size() {
    let rail = ledger::Facts {
        state: Some(sindri_core::run::State::Stopped(
            sindri_core::run::Stop::Cancelled,
        )),
        model: Some("a-model".into()),
        ..Default::default()
    };
    for (width, height) in [(100, 40), (80, 24)] {
        let mut view = View::of(
            caps::faced(false),
            Facts::default().asked().railed(rail.clone()),
            Transcript::none(),
        );
        let frame = frame::painted(&mut view, width, height);
        assert!(
            !frame
                .told()
                .sizes()
                .laid()
                .iter()
                .any(|laid| laid.run().scale() == 2)
        );
        if width == 80 {
            assert!(frame.told().sizes().laid().is_empty());
        }
    }
}

#[test]
fn the_state_headline_sizes_only_the_word_and_keeps_its_stop_mark() {
    let rail = ledger::Facts {
        state: Some(sindri_core::run::State::Stopped(
            sindri_core::run::Stop::Cancelled,
        )),
        ..Default::default()
    };
    let mut view = View::of(
        caps::faced(false),
        Facts::default().asked().railed(rail),
        Transcript::none(),
    );
    let frame = frame::painted(&mut view, 200, 40);
    let headline = frame
        .told()
        .sizes()
        .laid()
        .iter()
        .find(|laid| laid.run().scale() == 2)
        .expect("the state has room for a headline");
    assert_eq!(headline.run().words(), "cancelled");
    let (row, column) = frame.drawn_at("✗");
    assert_eq!(row, headline.row());
    assert!(column < headline.left());
    assert!(frame.told().sizes().covers(row, column).is_none());
}

#[test]
fn a_width_only_terminal_gets_the_unsized_rails_cells() {
    use sindri::capability::{Asked, Question};
    struct WidthOnly;
    impl Asked for WidthOnly {
        fn winsize(&self) -> Option<sindri::graphics::CellSize> {
            Some(sindri::graphics::CellSize::NOMINAL)
        }
        fn ask(&self, question: Question) -> Option<Vec<u8>> {
            match question {
                Question::Sizing => Some(b"\x1b[1;1R\x1b[1;2R".to_vec()),
                Question::Cell => None,
                Question::Animation => None,
            }
        }
    }
    let terms = std::collections::BTreeMap::from([
        ("TERM".into(), "xterm-kitty".into()),
        ("COLORTERM".into(), "truecolor".into()),
    ]);
    let partial = Capabilities::probed(&terms, &WidthOnly, false).0;
    let plain = caps::pictured().faced(false);
    let rail = ledger::Facts {
        state: Some(sindri_core::run::State::Running),
        model: Some("界-model".into()),
        ..Default::default()
    };
    let paint = |caps| {
        let mut view = View::of(
            caps,
            Facts::default().asked().railed(rail.clone()),
            Transcript::none(),
        );
        frame::painted(&mut view, 160, 40)
    };
    let partial = paint(partial);
    let plain = paint(plain);
    assert!(partial.told().sizes().laid().is_empty());
    assert_eq!(partial.cells(), plain.cells());
}

#[test]
fn submitted_prompt_surfaces_leave_the_transcripts_right_margin_on_the_well() {
    for palette in [Palette::TRUECOLOR, Palette::ANSI, Palette::SLOTS] {
        for (cols, rows) in [(80, 24), (120, 40), (40, 24)] {
            let mut entries = vec![Entry::Prompt {
                text: SecretSet::default().redact("a short prompt"),
            }];
            entries.extend(answer("an answer"));
            let mut view = View::of(caps::inked(palette), railed(), Transcript::of(entries));
            let frame = frame::painted(&mut view, cols, rows);
            let band = frame.told().layout().unwrap().transcript;
            let (y, _) = frame.drawn_at("a short prompt");
            for x in band.x..band.right() - 2 {
                assert_eq!(frame.cell(x, y).bg, palette.band(Band::Prompt));
            }
            for x in band.right() - 2..band.right() {
                assert_eq!(frame.cell(x, y).bg, palette.band(Band::Well));
            }
            let (y, _) = frame.drawn_at("an answer");
            for x in band.x..band.right() {
                assert_eq!(frame.cell(x, y).bg, palette.band(Band::Well));
            }
        }
    }
}

#[test]
fn submitted_prompt_surfaces_preserve_literal_rows_and_ownership_across_capabilities() {
    let words = "first line\n\n界界 wide words\n```rust\nlet literal = true;\n```";
    for (cols, rows) in [(80, 24), (120, 40), (40, 24), (12, 24)] {
        let mut reference = None;
        for capabilities in [
            caps::inked(Palette::TRUECOLOR),
            caps::pictured(),
            caps::inked(Palette::ANSI),
            caps::greys(),
            caps::inked(Palette::SLOTS),
            caps::uncoloured(),
        ] {
            let palette = capabilities.palette();
            let pictured = capabilities.pictures().draws() && palette != Palette::SLOTS;
            let mut entries = vec![Entry::Prompt {
                text: SecretSet::default().redact(words),
            }];
            entries.extend(answer("answer one"));
            entries.push(Entry::Prompt {
                text: SecretSet::default().redact("second prompt"),
            });
            entries.extend(answer("answer two"));
            let mut view = View::of(capabilities.faced(false), railed(), Transcript::of(entries));
            let frame = frame::painted(&mut view, cols, rows);
            let band = frame.told().layout().unwrap().transcript;
            let text: Vec<_> = (band.y..band.bottom())
                .map(|y| {
                    frame
                        .row(y)
                        .chars()
                        .take(usize::from(band.width))
                        .collect::<String>()
                })
                .collect();
            if let Some(reference) = &reference {
                assert_eq!(&text, reference);
            } else {
                reference = Some(text);
            }
            for y in band.y..band.bottom() {
                if frame.cell(band.x, y).symbol() == "┃" {
                    for x in band.x..band.right() - 2 {
                        assert_eq!(
                            frame.cell(x, y).bg,
                            if pictured {
                                ratatui::style::Color::Reset
                            } else {
                                palette.band(Band::Prompt)
                            }
                        );
                    }
                    for x in band.right() - 2..band.right() {
                        assert_eq!(
                            frame.cell(x, y).bg,
                            if pictured {
                                ratatui::style::Color::Reset
                            } else {
                                palette.band(Band::Well)
                            }
                        );
                    }
                }
            }
            if cols >= 40 {
                let (y, x) = frame.drawn_at("first line");
                assert_eq!(x, 2);
                assert_eq!(frame.cell(0, y).symbol(), "┃");
                assert_eq!(frame.cell(1, y).symbol(), "›");
                assert_eq!(frame.cell(0, y + 1).symbol(), "┃");
                assert!(frame.row(y + 3).contains("```rust"));
                let (answer_y, answer_x) = frame.drawn_at("answer one");
                assert_eq!(answer_x, x);
                assert_eq!(answer_y, y + 6);
            }
        }
    }
}

#[test]
fn submitted_prompt_text_clears_ordinary_text_contrast_on_its_actual_surface() {
    use sindri::palette::{LEGIBLE, channels};
    let luminance = |colour| {
        let [red, green, blue] = channels(colour).unwrap().map(|value| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * red + 0.7152 * green + 0.0722 * blue
    };
    for palette in [Palette::TRUECOLOR, Palette::ANSI] {
        let mut view = View::of(
            caps::inked(palette),
            railed(),
            Transcript::of(vec![Entry::Prompt {
                text: SecretSet::default().redact("ordinary prompt text"),
            }]),
        );
        let frame = frame::painted(&mut view, 120, 40);
        let (y, x) = frame.drawn_at("ordinary prompt text");
        for column in 0..x + 20 {
            let cell = frame.cell(column, y);
            let foreground = luminance(cell.fg);
            let background = luminance(cell.bg);
            let contrast =
                (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
            assert!(contrast >= if column < x { LEGIBLE } else { 4.5 });
        }
    }
}

#[test]
fn submitted_prompt_folds_keep_original_copy_and_reader_opening_after_reflow() {
    let words = (1..=16)
        .map(|line| format!("line {line}: literal 界 words and ``` fence"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut view = View::of(
        caps::pointing(),
        railed(),
        Transcript::of(vec![Entry::Prompt {
            text: SecretSet::default().redact(&words),
        }]),
    );
    let frame = frame::painted(&mut view, 120, 40);
    let (head, _) = frame.drawn_at("line 1:");
    let (count, _) = frame.drawn_at("12 rows kept back");
    view.drag(Cell::new(head, 0), Cell::new(count, 40));
    frame::painted(&mut view, 120, 40);
    assert_eq!(view.key(Key::Copy), Outcome::Copy(words));
    assert_eq!(
        view.pointer(frame.told(), Pointer::Press(Cell::new(count, 10))),
        Outcome::Edited
    );
    let opened = frame::painted(&mut view, 120, 40);
    assert!(opened.rows().iter().any(|row| row.contains("line 16:")));
    let reflowed = frame::painted(&mut view, 40, 24);
    assert!(
        !reflowed
            .rows()
            .iter()
            .any(|row| row.contains("rows kept back"))
    );
    let band = reflowed.told().layout().unwrap().transcript;
    assert_eq!(reflowed.cell(0, band.y).symbol(), "┃");
    for y in band.y..band.bottom() {
        for x in band.right() - 2..band.right() {
            assert_eq!(reflowed.cell(x, y).bg, Palette::TRUECOLOR.band(Band::Well));
        }
    }
}

#[test]
fn submitted_prompt_fold_counts_follow_width_and_closing_keeps_the_inset_surface() {
    let words = (1..=16)
        .map(|line| format!("line {line}: literal 界 words and ``` fence"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut view = View::of(
        caps::pointing(),
        railed(),
        Transcript::of(vec![Entry::Prompt {
            text: SecretSet::default().redact(words),
        }]),
    );
    frame::painted(&mut view, 120, 40).drawn_at("12 rows kept back");
    let narrow = frame::painted(&mut view, 40, 24);
    let (count, _) = narrow.drawn_at("28 rows kept back");
    assert_eq!(
        view.pointer(narrow.told(), Pointer::Press(Cell::new(count, 10))),
        Outcome::Edited
    );
    let wide = frame::painted(&mut view, 120, 40);
    let (head, _) = wide.drawn_at("line 1:");
    assert_eq!(
        view.pointer(wide.told(), Pointer::Press(Cell::new(head, 10))),
        Outcome::Edited
    );
    let closed = frame::painted(&mut view, 120, 40);
    let (count, _) = closed.drawn_at("12 rows kept back");
    let band = closed.told().layout().unwrap().transcript;
    for x in band.x..band.right() - 2 {
        assert_eq!(
            closed.cell(x, count).bg,
            Palette::TRUECOLOR.band(Band::Prompt)
        );
    }
}

#[test]
fn illuminated_prompts_keep_native_words_and_independent_unmasked_surfaces() {
    let entries = || {
        Transcript::of(vec![
            Entry::Prompt {
                text: SecretSet::default().redact("first 界 prompt"),
            },
            Entry::Prompt {
                text: SecretSet::default().redact("second prompt"),
            },
        ])
    };
    let mut lit = View::of(caps::pictured(), railed(), entries());
    let mut flat = View::of(caps::inked(Palette::TRUECOLOR), railed(), entries());
    let lit_frame = frame::painted(&mut lit, 80, 24);
    let flat_frame = frame::painted(&mut flat, 80, 24);
    assert_eq!(lit_frame.words(), flat_frame.words());
    let mut held = sindri::plane::Held::default();
    let wire = String::from_utf8(held.pass(lit_frame.told().placements(), lit.planes())).unwrap();
    assert_eq!(
        wire.matches("a=T").count(),
        lit_frame.told().placements().len()
    );
    for words in ["first 界 prompt", "second prompt"] {
        let (y, x) = lit_frame.drawn_at(if words.starts_with("first") {
            "first"
        } else {
            words
        });
        assert_eq!(lit_frame.cell(x, y).bg, ratatui::style::Color::Reset);
    }
    assert!(
        held.pass(lit_frame.told().placements(), lit.planes())
            .is_empty()
    );
}

#[test]
fn illuminated_prompts_scroll_by_placement_then_release_and_return_without_ghosts() {
    let mut entries = vec![Entry::Prompt {
        text: SecretSet::default().redact("landmark prompt"),
    }];
    for n in 0..60 {
        entries.push(Entry::Prompt {
            text: SecretSet::default().redact(format!("later prompt {n}")),
        });
    }
    let mut view = View::of(caps::pictured(), railed(), Transcript::of(entries));
    let mut held = sindri::plane::Held::default();
    frame::painted(&mut view, 80, 24);
    while view.key(Key::PageUp) == Outcome::Edited {
        frame::painted(&mut view, 80, 24);
    }
    let top = frame::painted(&mut view, 80, 24);
    assert!(
        top.words()
            .iter()
            .any(|row| row.contains("landmark prompt"))
    );
    held.pass(top.told().placements(), view.planes());
    view.key(Key::PageDown);
    let away = frame::painted(&mut view, 80, 24);
    let wire = String::from_utf8(held.pass(away.told().placements(), view.planes())).unwrap();
    assert!(
        wire.contains("a=d,d=I"),
        "offscreen Prompt resources are released: {wire}"
    );
    view.key(Key::PageUp);
    let back = frame::painted(&mut view, 80, 24);
    let wire = String::from_utf8(held.pass(back.told().placements(), view.planes())).unwrap();
    assert!(wire.contains("a=T"));
    assert!(
        String::from_utf8(held.forget())
            .unwrap()
            .contains("a=d,d=I")
    );
    assert!(held.forget().is_empty());
}

#[test]
fn illuminated_prompt_clipping_moves_the_original_pixels_and_preserves_end_corners() {
    let words = (0..18)
        .map(|n| format!("literal row {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut entries = vec![Entry::Prompt {
        text: SecretSet::default().redact(words),
    }];
    for n in 0..12 {
        entries.push(Entry::Prompt {
            text: SecretSet::default().redact(format!("tail {n}")),
        });
    }
    let mut view = View::of(caps::pictured(), railed(), Transcript::of(entries));
    let mut held = sindri::plane::Held::default();
    frame::painted(&mut view, 80, 24);
    while view.key(Key::PageUp) == Outcome::Edited {
        frame::painted(&mut view, 80, 24);
    }
    let top = frame::painted(&mut view, 80, 24);
    let band = top.told().layout().unwrap().transcript;
    let (y, x) = top.drawn_at("literal row 0");
    view.pointer(top.told(), Pointer::Press(Cell::new(y, x)));
    let opened = frame::painted(&mut view, 80, 24);
    let surfaces: Vec<_> = opened
        .told()
        .placements()
        .iter()
        .filter(|p| matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_)))
        .collect();
    assert!(!surfaces.is_empty());
    for p in &surfaces {
        assert_eq!(p.cells().intersection(band), p.cells());
    }
    held.pass(opened.told().placements(), view.planes());
    view.pointer(opened.told(), Pointer::Wheel(sindri::pointer::Notch::Down));
    let moved = frame::painted(&mut view, 80, 24);
    let wire = String::from_utf8(held.pass(moved.told().placements(), view.planes())).unwrap();
    assert!(
        wire.contains("a=p"),
        "scrolling a still-visible surface reuses its pixels"
    );
    let prompt = moved
        .told()
        .placements()
        .iter()
        .find(|p| {
            matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_))
                && p.shows().is_some_and(|crop| crop.y > 0)
        })
        .unwrap();
    assert!(prompt.shows().unwrap().y > 0);
    let plane = view.planes().image(prompt.shade()).unwrap();
    assert_ne!(
        plane.pixels().get_pixel(0, 0),
        plane.pixels().get_pixel(4, 4)
    );
    for pixel in plane.pixels().pixels() {
        let color = ratatui::style::Color::Rgb(pixel[0], pixel[1], pixel[2]);
        let luminance = |color| {
            let c = sindri::palette::channels(color).unwrap().map(|v| {
                let v = f64::from(v) / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            });
            c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
        };
        let ink = luminance(opened.palette().ink(Ink::Plain).0);
        let surface = luminance(color);
        assert!((ink.max(surface) + 0.05) / (ink.min(surface) + 0.05) >= 4.5);
    }
}

#[test]
fn illuminated_prompts_fall_back_without_colour_and_draft_edits_send_no_surface_payload() {
    for caps in [
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::uncoloured(),
    ] {
        let mut view = View::of(
            caps,
            railed(),
            Transcript::of([Entry::Prompt {
                text: SecretSet::default().redact("fallback prompt"),
            }]),
        );
        let frame = frame::painted(&mut view, 80, 24);
        assert!(
            !frame
                .told()
                .placements()
                .iter()
                .any(|p| matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_)))
        );
        assert!(
            frame
                .words()
                .iter()
                .any(|row| row.contains("fallback prompt"))
        );
    }
    let mut view = View::of(
        caps::pictured(),
        railed(),
        Transcript::of([Entry::Prompt {
            text: SecretSet::default().redact("static prompt"),
        }]),
    );
    let mut held = sindri::plane::Held::default();
    let first = frame::painted(&mut view, 80, 24);
    held.pass(first.told().placements(), view.planes());
    view.key(Key::Char('a'));
    let edited = frame::painted(&mut view, 80, 24);
    let wire = String::from_utf8(held.pass(edited.told().placements(), view.planes())).unwrap();
    assert!(!wire.contains("a=T"));
}

#[test]
fn illuminated_prompt_opening_keeps_original_copy_and_replaces_only_its_surface() {
    let words = (1..=16)
        .map(|n| format!("literal 界 row {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut view = View::of(
        caps::pictured(),
        railed(),
        Transcript::of([
            Entry::Prompt {
                text: SecretSet::default().redact(&words),
            },
            Entry::Prompt {
                text: SecretSet::default().redact("unchanged landmark"),
            },
        ]),
    );
    let mut held = sindri::plane::Held::default();
    let closed = frame::painted(&mut view, 120, 40);
    closed.drawn_at("12 rows kept back");
    held.pass(closed.told().placements(), view.planes());
    let (y, x) = closed.drawn_at("12 rows kept back");
    assert_eq!(
        view.pointer(closed.told(), Pointer::Press(Cell::new(y, x))),
        Outcome::Edited
    );
    let opened = frame::painted(&mut view, 120, 40);
    opened.drawn_at("row 16");
    let wire = String::from_utf8(held.pass(opened.told().placements(), view.planes())).unwrap();
    assert_eq!(wire.matches("a=d,d=I").count(), 1);
    assert_eq!(wire.matches("a=T").count(), 1);
    assert!(wire.contains("a=p"));
    let (head, _) = opened.drawn_at("row 1");
    let (tail, _) = opened.drawn_at("row 16");
    view.drag(Cell::new(head, 0), Cell::new(tail, 60));
    frame::painted(&mut view, 120, 40);
    assert_eq!(view.key(Key::Copy), Outcome::Copy(words));
    let narrow = frame::painted(&mut view, 40, 24);
    assert!(
        !narrow
            .rows()
            .iter()
            .any(|row| row.contains("rows kept back"))
    );
    let wide = frame::painted(&mut view, 120, 40);
    let (head, x) = wide.drawn_at("row 1");
    view.pointer(wide.told(), Pointer::Press(Cell::new(head, x)));
    let folded = frame::painted(&mut view, 120, 40);
    folded.drawn_at("12 rows kept back");
    let wire = String::from_utf8(held.pass(folded.told().placements(), view.planes())).unwrap();
    assert_eq!(wire.matches("a=d,d=I").count(), 1);
    assert_eq!(wire.matches("a=T").count(), 1);
}

#[test]
fn illuminated_prompt_resize_and_display_changes_keep_cells_aligned_and_release_old_pixels() {
    let words = (1..=16)
        .map(|n| format!("row {n}: literal words long enough to wrap at forty columns"))
        .collect::<Vec<_>>()
        .join("\n");
    let entries = || {
        Transcript::of([Entry::Prompt {
            text: SecretSet::default().redact(&words),
        }])
    };
    let mut view = View::of(caps::pictured(), railed(), entries());
    let mut flat = View::of(caps::inked(Palette::TRUECOLOR), railed(), entries());
    let mut held = sindri::plane::Held::default();
    let first = frame::painted(&mut view, 120, 40);
    first.drawn_at("12 rows kept back");
    held.pass(first.told().placements(), view.planes());
    for (cols, rows, count) in [
        (80, 24, "12 rows kept back"),
        (40, 24, "28 rows kept back"),
        (120, 40, "12 rows kept back"),
    ] {
        let resized = frame::painted(&mut view, cols, rows);
        let text = frame::painted(&mut flat, cols, rows);
        let band = resized.told().layout().unwrap().transcript;
        for y in band.y..band.bottom() {
            for x in band.x..band.right() {
                assert_eq!(resized.cell(x, y).symbol(), text.cell(x, y).symbol());
            }
        }
        assert!(
            resized.rows().iter().any(|r| r.contains(count)),
            "{cols}: {:?}",
            resized.rows()
        );
        let wire =
            String::from_utf8(held.pass(resized.told().placements(), view.planes())).unwrap();
        assert!(wire.contains("a=d,d=I"));
        assert!(wire.contains("a=T"));
        assert!(
            held.pass(resized.told().placements(), view.planes())
                .is_empty()
        );
    }
    let before = frame::painted(&mut view, 120, 40);
    let taller = frame::painted(&mut view, 120, 48);
    let wire = String::from_utf8(held.pass(taller.told().placements(), view.planes())).unwrap();
    assert!(!wire.contains("a=d,d=I"));
    let prompt = taller
        .told()
        .placements()
        .iter()
        .find(|p| matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_)))
        .unwrap();
    let crop = prompt.shows().unwrap();
    let surface = format!(
        ",c={},r={},x=0,y=0,w={},h={}",
        prompt.cells().width,
        prompt.cells().height,
        crop.width,
        crop.height
    );
    assert!(
        !wire
            .split("\x1b_G")
            .any(|op| op.starts_with("a=T") && op.contains(&surface))
    );
    assert_eq!(
        before
            .words()
            .iter()
            .filter(|r| r.contains("row 1:"))
            .count(),
        1
    );
    view.celled(Some(sindri::graphics::CellSize::new(12, 24)));
    let scaled = frame::painted(&mut view, 120, 48);
    assert_eq!(scaled.words(), taller.words());
    let wire = String::from_utf8(held.pass(scaled.told().placements(), view.planes())).unwrap();
    assert!(wire.contains("a=d,d=I"));
    assert!(wire.contains("a=T"));
    for p in scaled
        .told()
        .placements()
        .iter()
        .filter(|p| matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_)))
    {
        let crop = p.shows().unwrap();
        assert_eq!(crop.width, u32::from(p.cells().width) * 12);
        assert_eq!(crop.height, u32::from(p.cells().height) * 24);
    }
    for caps in [
        caps::greys(),
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::uncoloured(),
        caps::pictured(),
    ] {
        view.drawn_on(caps.clone());
        flat.drawn_on(caps);
        let changed = frame::painted(&mut view, 80, 24);
        let reference = frame::painted(&mut flat, 80, 24);
        assert_eq!(changed.words(), reference.words());
        changed.drawn_at("12 rows kept back");
        let wire =
            String::from_utf8(held.pass(changed.told().placements(), view.planes())).unwrap();
        if !changed
            .told()
            .placements()
            .iter()
            .any(|p| matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_)))
        {
            assert!(!wire.contains("a=T"));
            let (y, x) = changed.drawn_at("row 1:");
            assert_eq!(changed.cell(x, y).bg, changed.palette().band(Band::Prompt));
        } else {
            assert!(wire.contains("a=T"));
        }
    }
}

#[test]
fn short_prompt_copy_returns_original_blank_lines_and_literal_words_in_every_fallback() {
    let words = "界 literal ```\n\n  indented words";
    for caps in [
        caps::pictured(),
        caps::inked(Palette::TRUECOLOR),
        caps::inked(Palette::ANSI),
        caps::uncoloured(),
    ] {
        let mut view = View::of(
            caps,
            railed(),
            Transcript::of([Entry::Prompt {
                text: SecretSet::default().redact(words),
            }]),
        );
        let frame = frame::painted(&mut view, 80, 24);
        let (head, _) = frame.drawn_at("literal");
        let (tail, _) = frame.drawn_at("indented words");
        view.drag(Cell::new(head, 0), Cell::new(tail, 50));
        frame::painted(&mut view, 80, 24);
        assert_eq!(view.key(Key::Copy), Outcome::Copy(words.to_owned()));
    }
}

#[test]
fn partially_visible_open_prompt_stays_clipped_through_resize_scale_and_fallback() {
    let words = (1..=60)
        .map(|n| format!("visible literal row {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut view = View::of(
        caps::pictured(),
        railed(),
        Transcript::of([Entry::Prompt {
            text: SecretSet::default().redact(words),
        }]),
    );
    let closed = frame::painted(&mut view, 80, 24);
    let (y, x) = closed.drawn_at("56 rows kept back");
    assert_eq!(
        view.pointer(closed.told(), Pointer::Press(Cell::new(y, x))),
        Outcome::Edited
    );
    let mut held = sindri::plane::Held::default();
    for (cols, rows, cell) in [(80, 24, (10, 20)), (120, 40, (12, 24)), (40, 16, (8, 16))] {
        view.celled(Some(sindri::graphics::CellSize::new(cell.0, cell.1)));
        let frame = frame::painted(&mut view, cols, rows);
        assert!(!frame.words().iter().any(|r| r.contains("rows kept back")));
        let band = frame.told().layout().unwrap().transcript;
        let surfaces: Vec<_> = frame
            .told()
            .placements()
            .iter()
            .filter(|p| matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_)))
            .collect();
        assert_eq!(surfaces.len(), 1);
        let p = surfaces[0];
        assert_eq!(p.cells().intersection(band), p.cells());
        assert!(p.shows().unwrap().y > 0);
        assert_eq!(
            p.shows().unwrap().height,
            u32::from(p.cells().height) * cell.1
        );
        let wire = String::from_utf8(held.pass(frame.told().placements(), view.planes())).unwrap();
        assert!(wire.contains("a=T"));
        assert!(
            held.pass(frame.told().placements(), view.planes())
                .is_empty()
        );
    }
    view.drawn_on(caps::uncoloured());
    let fallback = frame::painted(&mut view, 40, 16);
    let wire = String::from_utf8(held.pass(fallback.told().placements(), view.planes())).unwrap();
    assert_eq!(wire.matches("a=d,d=I").count(), 1);
    assert!(!wire.contains("a=T"));
    assert!(
        !fallback
            .words()
            .iter()
            .any(|r| r.contains("rows kept back"))
    );
    fallback.drawn_at("visible literal row 60");
}

#[test]
fn prompt_folds_and_copy_match_in_the_complete_terminal_matrix() {
    let words = (1..=16)
        .map(|n| format!("line {n}: literal 界 words and ``` fence"))
        .collect::<Vec<_>>()
        .join("\n");
    for capabilities in [
        caps::pictured(),
        caps::inked(Palette::TRUECOLOR),
        caps::greys(),
        caps::inked(Palette::ANSI),
        caps::uncoloured(),
        caps::inked(Palette::SLOTS),
    ] {
        for (cols, rows, count) in [
            (80, 24, "12 rows kept back"),
            (120, 40, "12 rows kept back"),
            (40, 24, "28 rows kept back"),
        ] {
            let mut view = View::of(
                capabilities.clone().faced(false),
                railed(),
                Transcript::of([Entry::Prompt {
                    text: SecretSet::default().redact(&words),
                }]),
            );
            let closed = frame::painted(&mut view, cols, rows);
            let (head, _) = closed.drawn_at("line 1:");
            let (tail, _) = closed.drawn_at(count);
            view.drag(Cell::new(head, 0), Cell::new(tail, cols - 3));
            frame::painted(&mut view, cols, rows);
            assert_eq!(view.key(Key::Copy), Outcome::Copy(words.clone()));
            assert_eq!(view.key(Key::Fold), Outcome::Edited);
            let opened = frame::painted(&mut view, 120, 40);
            opened.drawn_at("line 16:");
            let narrow = frame::painted(&mut view, 40, 24);
            assert!(!narrow.rows().iter().any(|r| r.contains("rows kept back")));
            let wide = frame::painted(&mut view, 120, 40);
            wide.drawn_at("line 1:");
            assert_eq!(view.key(Key::Fold), Outcome::Edited);
            frame::painted(&mut view, cols, rows).drawn_at(count);
        }
    }
}

#[test]
fn long_conversations_release_visible_prompt_resources_on_every_scroll_and_resize() {
    use std::collections::BTreeSet;
    for count in [100, 1000] {
        let entries = (0..count).flat_map(|n| {
            [
                Entry::Prompt {
                    text: SecretSet::default().redact(format!("landmark {n} 界")),
                },
                Entry::Assistant {
                    turn: 1,
                    text: SecretSet::default().redact("calm answer"),
                },
            ]
        });
        let mut view = View::of(caps::pictured(), railed(), Transcript::of(entries));
        let mut held = sindri::plane::Held::default();
        let mut live = BTreeSet::new();
        for pass in 0..80 {
            let (cols, rows) = [(80, 24), (120, 40), (40, 24)][pass % 3];
            view.key(if pass < 40 {
                Key::PageUp
            } else {
                Key::PageDown
            });
            let frame = frame::painted(&mut view, cols, rows);
            let band = frame.told().layout().unwrap().transcript;
            for p in frame
                .told()
                .placements()
                .iter()
                .filter(|p| matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_)))
            {
                assert!(band.contains(Position::new(p.cells().x, p.cells().y)));
                assert!(p.cells().right() <= band.right() - 2);
                assert!(p.cells().bottom() <= band.bottom());
            }
            let wire =
                String::from_utf8(held.pass(frame.told().placements(), view.planes())).unwrap();
            let mut uploads = 0;
            for op in wire.split("\x1b_G").skip(1) {
                let head = op.split([';', '\x1b']).next().unwrap();
                let id = head.split(',').find_map(|field| field.strip_prefix("i="));
                if let Some(id) = id {
                    if head.starts_with("a=T,") {
                        assert!(live.insert(id.to_owned()));
                        uploads += 1;
                    } else if head.starts_with("a=d,") {
                        assert!(live.remove(id));
                    } else if head.starts_with("a=p,") {
                        assert!(live.contains(id));
                    }
                }
            }
            assert_eq!(
                live.len(),
                frame
                    .told()
                    .placements()
                    .iter()
                    .filter(|p| p.places())
                    .count()
            );
            assert!(live.len() <= usize::from(band.height) + 4);
            assert!(uploads <= live.len());
            assert!(
                held.pass(frame.told().placements(), view.planes())
                    .is_empty()
            );
        }
        let cleanup = String::from_utf8(held.forget()).unwrap();
        assert_eq!(cleanup.matches("a=d,").count(), live.len());
        assert!(held.forget().is_empty());
    }
}

#[test]
fn every_ink_keeps_its_floor_on_visible_prompt_answer_and_rail_pixels() {
    use sindri::palette::{LEGIBLE, channels};
    let luminance = |rgb: [u8; 3]| {
        let [r, g, b] = rgb.map(|v| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    for capabilities in [caps::pictured(), caps::greys()] {
        let palette = capabilities.palette();
        let mut view = View::of(
            capabilities,
            railed(),
            Transcript::of([
                Entry::Prompt {
                    text: SecretSet::default().redact("ordinary prompt 界"),
                },
                Entry::Assistant {
                    turn: 1,
                    text: SecretSet::default().redact("calm answer"),
                },
            ]),
        );
        let frame = frame::painted(&mut view, 120, 40);
        for placement in frame.told().placements() {
            let plane = view.planes().image(placement.shade()).unwrap();
            let prompt = matches!(placement.shade().id(), sindri::plane::PlaneId::Prompt(_));
            let cell = sindri::graphics::CellSize::NOMINAL;
            for (x, y, pixel) in plane.pixels().enumerate_pixels() {
                if placement.seam().is_some_and(|(axis, _)| match axis {
                    sindri::plane::Axis::Down => x < cell.width(),
                    sindri::plane::Axis::Across => y < cell.height(),
                }) {
                    continue;
                }
                if pixel[3] == 0 {
                    continue;
                }
                let bg = luminance([pixel[0], pixel[1], pixel[2]]);
                for ink in Ink::ALL {
                    let Some(rgb) = channels(palette.colour(ink)) else {
                        continue;
                    };
                    let fg = luminance(rgb);
                    let ratio = (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05);
                    let floor = if prompt && ink == Ink::Plain {
                        4.5
                    } else {
                        LEGIBLE
                    };
                    assert!(
                        ratio >= floor,
                        "{ink:?} on {:?}: {ratio}",
                        placement.shade().id()
                    );
                }
            }
        }
    }
}

#[test]
fn huge_opened_prompt_uploads_are_bounded_by_the_visible_band_through_scroll_and_resize() {
    let words = (0..60_000)
        .map(|n| format!("row {n}\n"))
        .collect::<String>();
    let mut view = View::of(
        caps::pictured(),
        railed(),
        Transcript::of([Entry::Prompt {
            text: SecretSet::default().redact(words),
        }]),
    );
    let folded = frame::painted(&mut view, 120, 40);
    let (y, x) = folded.drawn_at("rows kept back");
    assert_eq!(
        view.pointer(folded.told(), Pointer::Press(Cell::new(y, x))),
        Outcome::Edited
    );
    let mut held = sindri::plane::Held::default();
    for (cols, rows) in [(120, 40), (120, 40), (80, 24), (40, 24), (120, 48)] {
        for pass in 0..12 {
            let frame = frame::painted(&mut view, cols, rows);
            let band = frame.told().layout().unwrap().transcript;
            for placement in frame
                .told()
                .placements()
                .iter()
                .filter(|p| matches!(p.shade().id(), sindri::plane::PlaneId::Prompt(_)))
            {
                assert_eq!(placement.cells().intersection(band), placement.cells());
                let image = view.planes().image(placement.shade()).unwrap().pixels();
                assert!(image.width() <= u32::from(band.width) * 10);
                assert!(
                    image.height() <= u32::from(band.height) * 20 * 3,
                    "Prompt image height {} exceeds visible-band bound",
                    image.height()
                );
            }
            let wire = held.pass(frame.told().placements(), view.planes());
            assert!(wire.len() <= usize::from(cols) * usize::from(rows) * 10 * 20 * 16 + 100_000);
            use base64::Engine;
            let text = std::str::from_utf8(&wire).unwrap();
            let mut payload = String::new();
            let mut uploads = 0;
            for op in text.split("\x1b_G").skip(1) {
                let command = op.split("\x1b\\").next().unwrap();
                let Some((head, chunk)) = command.split_once(';') else {
                    continue;
                };
                payload.push_str(chunk);
                if head.split(',').any(|field| field == "m=0") {
                    let png = base64::engine::general_purpose::STANDARD
                        .decode(&payload)
                        .unwrap();
                    let image = image::load_from_memory(&png).unwrap();
                    assert!(image.width() <= u32::from(cols) * 10);
                    assert!(image.height() <= u32::from(rows) * 20 * 3);
                    assert!(png.len() <= usize::from(cols) * usize::from(rows) * 10 * 20 * 13);
                    payload.clear();
                    uploads += 1;
                }
            }
            assert!(payload.is_empty());
            if pass == 0 {
                assert!(uploads > 0 || !wire.is_empty());
            }
            assert!(
                held.pass(frame.told().placements(), view.planes())
                    .is_empty()
            );
            if pass % 3 == 0 {
                view.key(Key::PageUp);
            } else {
                view.pointer(frame.told(), Pointer::Wheel(sindri::pointer::Notch::Up));
            }
        }
    }
}

fn disclosed_result(exit: sindri_core::run::Exit, text: &str) -> Vec<Entry> {
    let secrets = SecretSet::default();
    vec![
        Entry::Call {
            turn: 1,
            id: None,
            name: secrets.redact("bash"),
            arguments: secrets.redact(r#"{"command":"make check"}"#),
        },
        Entry::Result {
            turn: 1,
            id: None,
            name: secrets.redact("bash"),
            text: secrets.redact(text),
            bytes: text.len() as u64,
            elapsed_ms: 2,
            exit: Some(exit),
        },
    ]
}

#[test]
fn failed_result_discloses_recorded_diagnostic_before_its_output_fold() {
    let text = "building\n\n  target    state\n  one       waiting\n  two       waiting\n\nerror: permission denied\n";
    for capabilities in [
        caps::pictured(),
        caps::inked(Palette::TRUECOLOR),
        caps::greys(),
        caps::inked(Palette::ANSI),
        caps::uncoloured(),
        caps::inked(Palette::SLOTS),
    ] {
        let mut view = View::of(
            capabilities.faced(false),
            railed(),
            Transcript::of(disclosed_result(sindri_core::run::Exit::Code(1), text)),
        );
        let closed = frame::painted(&mut view, 120, 40);
        let (diagnostic, _) = closed.drawn_at("error: permission denied");
        let (fold, _) = closed.drawn_at("4 rows kept back");
        assert!(diagnostic < fold);
    }
}

#[test]
fn result_fold_controls_preserve_reader_choice_and_original_source_across_reflow() {
    let text = "first\n\n  target    state\n  one       waiting\n  two       waiting\n\nlast 界\n";
    for capabilities in [
        caps::pictured(),
        caps::inked(Palette::TRUECOLOR),
        caps::greys(),
        caps::inked(Palette::ANSI),
        caps::uncoloured(),
        caps::inked(Palette::SLOTS),
    ] {
        let pointing = capabilities.reports_pointer();
        let mut view = View::of(
            capabilities.faced(false),
            railed(),
            Transcript::of(disclosed_result(sindri_core::run::Exit::Code(0), text)),
        );
        let closed = frame::painted(&mut view, 120, 40);
        let (head, _) = closed.drawn_at("first");
        let (tail, _) = closed.drawn_at("4 rows kept back");
        view.drag(Cell::new(head, 0), Cell::new(tail, 110));
        frame::painted(&mut view, 120, 40);
        assert_eq!(view.key(Key::Copy), Outcome::Copy(text.to_owned()));
        assert_eq!(view.key(Key::Fold), Outcome::Edited);
        for (cols, rows) in [(80, 24), (40, 24), (12, 24), (120, 40)] {
            let open = frame::painted(&mut view, cols, rows);
            assert!(!open.rows().iter().any(|row| row.contains("rows kept back")));
        }
        assert_eq!(view.key(Key::Fold), Outcome::Edited);
        let closed = frame::painted(&mut view, 120, 40);
        let (y, x) = closed.drawn_at("4 rows kept back");
        if !pointing {
            assert_eq!(
                view.pointer(closed.told(), Pointer::Press(Cell::new(y, x))),
                Outcome::Unchanged
            );
            continue;
        }
        assert_eq!(
            view.pointer(closed.told(), Pointer::Press(Cell::new(y, x))),
            Outcome::Edited
        );
        let open = frame::painted(&mut view, 120, 40);
        open.drawn_at("last 界");
        let (y, x) = open.drawn_at("first");
        assert_eq!(
            view.pointer(open.told(), Pointer::Press(Cell::new(y, x))),
            Outcome::Edited
        );
        frame::painted(&mut view, 120, 40).drawn_at("4 rows kept back");
    }
}

#[test]
fn reopening_settled_exchange_keeps_its_individual_result_disclosure() {
    let text = "first\nsecond\nthird\nfourth\nfifth\nsixth\nseventh\neighth";
    let mut entries = vec![Entry::Prompt {
        text: SecretSet::default().redact("check this"),
    }];
    entries.extend(disclosed_result(sindri_core::run::Exit::Code(0), text));
    entries.extend(answer("The answer remains here."));
    entries.push(Entry::Stop {
        stop: sindri_core::run::Stop::Final,
    });
    let mut view = View::of(
        caps::pictured().faced(false),
        railed(),
        Transcript::of(entries),
    );
    frame::painted(&mut view, 120, 40).drawn_at("1 call");
    assert_eq!(view.key(Key::Fold), Outcome::Edited);
    let closed = frame::painted(&mut view, 120, 40);
    let (y, x) = closed.drawn_at("4 rows kept back");
    assert_eq!(
        view.pointer(closed.told(), Pointer::Press(Cell::new(y, x))),
        Outcome::Edited
    );
    frame::painted(&mut view, 120, 40).drawn_at("eighth");
    assert_eq!(view.key(Key::Fold), Outcome::Edited);
    frame::painted(&mut view, 40, 24).drawn_at("1 call");
    assert_eq!(view.key(Key::Fold), Outcome::Edited);
    let reopened = frame::painted(&mut view, 120, 40);
    reopened.drawn_at("eighth");
    assert!(
        !reopened
            .rows()
            .iter()
            .any(|row| row.contains("rows kept back"))
    );
}
