//! The rail reads as a ledger: values hanging off one edge, a dot leader
//! bridging the label's field to the figure where there is a gap to bridge, a
//! hairline between the blocks, and a dotted seam that begins and ends with
//! the facts (ADR-0011 §22, §23).

use super::*;

use ratatui::buffer::Buffer;
use sindri_tui::theme;

/// The nine columns a label sits in, once the seam and its space are past.
const FIELD: std::ops::Range<usize> = 2..11;
/// Where the room for a gap and a value starts: the rail's edge, the seam, the
/// label's field, and the space after it.
const LEDGER: usize = 12;

fn spent() -> App {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: "Two files changed.".to_string(),
    });
    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        tokens: Tokens {
            input: 12_400,
            output: 2_100,
            cached: 10_168,
        },
        error: None,
        note: None,
    });
    app
}

fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

#[test]
fn every_value_hangs_off_the_rails_last_column() {
    // The figures share one edge, wherever they start and however wide the
    // terminal is, so they can be compared down the column.
    for width in [100u16, 160] {
        let mut app = spent();
        let rows = rails(&screen(&mut app, width, 26), width);
        for (label, said) in [
            ("cache", "82%"),
            ("turns", "1/250"),
            ("tokens", "12.4k in · 2.1k out"),
            ("context", "░░░░░░░░ 12.4k/200k"),
            ("stop", "final"),
            ("model", "claude-sonnet-4-6"),
            ("workspace", "~/git/sindri"),
            ("branch", "t15-the-frame"),
        ] {
            let row = fact(&rows, label);
            let cells: Vec<char> = row.chars().collect();
            assert_eq!(cells.len(), RAIL, "{width} columns: {label:?}: {row:?}");
            assert_eq!(
                cells[FIELD.clone()].iter().collect::<String>(),
                format!("{label:<9}"),
                "{width} columns: {label:?} keeps the nine-column field: {row:?}"
            );
            assert_eq!(cells[11], ' ', "{width} columns: {label:?}: {row:?}");
            assert!(
                row.ends_with(said),
                "{width} columns: {label:?}'s value ends on the rail's last column: {row:?}"
            );
        }
    }
}

#[test]
fn dot_leaders_bridge_the_label_field_to_the_value() {
    let mut app = spent();
    let rows = rails(&screen(&mut app, 100, 26), 100);
    // `cache` is three cells of value in a twenty-cell ledger: seventeen cells
    // of gap, and every one of them a dot.
    let row = fact(&rows, "cache");
    let gap: String = row.chars().skip(LEDGER).take(17).collect();
    assert_eq!(
        gap,
        "·".repeat(17),
        "the gap between the field and the figure is dots: {row:?}"
    );
    assert_eq!(
        row.chars().count(),
        LEDGER + 17 + 3,
        "and nothing else is on the row: {row:?}"
    );
    // A value that leaves a cell or two of gap has nothing worth bridging:
    // `context`'s meter fills nineteen of the twenty cells, so it sits one
    // space off the field and carries no dots at all.
    let meter = fact(&rows, "context");
    assert_eq!(
        meter.chars().nth(LEDGER),
        Some(' '),
        "a gap under the floor is left blank: {meter:?}"
    );
    assert!(
        !meter.contains('·'),
        "and no leader is drawn on it: {meter:?}"
    );

    // The leader is the dimmer of the two: a run of dots reads heavier than a
    // word, so it may not outshine the label.
    let buf = buffer(&mut app, 100, 26);
    let left = (100 - RAIL) as u16;
    let cache = (0..buf.area.height)
        .find(|y| buf[(left + FIELD.start as u16, *y)].symbol() == "c")
        .expect("the rail counts what the run spent");
    assert_eq!(buf[(left + LEDGER as u16, cache)].fg, theme::LEADER);
    assert_eq!(buf[(left + FIELD.start as u16, cache)].fg, theme::FAINT);
}

#[test]
fn a_value_too_long_for_its_room_is_shortened_and_draws_no_leader() {
    // A path longer than the ledger's twenty cells leaves no gap to bridge: it
    // is shortened by `Fit`, and nothing wraps.
    let mut app = App::new(
        Launch {
            workspace: "~/git/sindri/a/long/way/down".to_string(),
            ..launch()
        },
        None,
    );
    let rows = rails(&screen(&mut app, 100, 26), 100);
    let row = fact(&rows, "workspace");
    assert!(
        row.contains('…') && row.ends_with("way/down"),
        "the value keeps both its ends and is shortened in the middle: {row:?}"
    );
    assert!(
        !row.contains('·'),
        "a leader with nothing to bridge is noise: {row:?}"
    );
    assert_eq!(
        rail_rows(&rows)
            .iter()
            .filter(|row| row.contains("~/git"))
            .count(),
        1,
        "nothing wraps onto a second row: {rows:#?}"
    );
}

/// Every row the rail drew: the ones that are not blank on its own columns.
fn rail_rows(rows: &[String]) -> Vec<String> {
    rows.iter()
        .filter(|row| !row.trim().is_empty())
        .cloned()
        .collect()
}

#[test]
fn the_block_rule_spans_the_rails_width_as_a_hairline() {
    let mut app = spent();
    let buf = buffer(&mut app, 100, 26);
    let left = (100 - RAIL) as u16;
    let hairline = (0..buf.area.height)
        .find(|y| buf[(left + 1, *y)].symbol() == "─")
        .expect("the identity block opens with a rule");
    let rule: String = (left..100)
        .map(|x| buf[(x, hairline)].symbol().to_string())
        .collect();
    assert_eq!(
        rule,
        format!("⁞{}", "─".repeat(RAIL - 1)),
        "the rule spans the rail's width, under the seam: {rule:?}"
    );
    for x in left + 1..100 {
        assert_eq!(
            buf[(x, hairline)].fg,
            theme::FAINT,
            "the hairline is faint: ({x},{hairline})"
        );
    }
    // It groups the blocks rather than dividing the column from the page: the
    // numbers above it, what the run is below it.
    assert_eq!(
        buf[(left, hairline - 1)].symbol(),
        "⁞",
        "the rule is between two facts, not above the first"
    );
}

#[test]
fn the_seam_is_a_dotted_spine_that_begins_and_ends_with_the_facts() {
    let mut app = spent();
    let buf = buffer(&mut app, 100, 40);
    let left = (100 - RAIL) as u16;
    let full = screen(&mut app, 100, 40);
    let rows = rails(&full, 100);
    // The rail's own rows: the composer below them draws a border the whole
    // width of the frame, and that border is not the rail.
    let body = full
        .iter()
        .position(|row| row.contains('╭'))
        .expect("the composer draws");
    let seam: Vec<u16> = (0..body as u16)
        .filter(|y| buf[(left, *y)].symbol() == "⁞")
        .collect();
    let first = *seam.first().expect("the rail has a top");
    let last = *seam.last().expect("the rail has a bottom");
    assert_eq!(
        seam.len() as u16,
        last - first + 1,
        "the seam is unbroken from its top to its bottom: {seam:?}"
    );
    assert_eq!(first, 0, "no face: the seam starts on the rail's first row");
    assert!(
        rows[last as usize].contains("⬢ bwrap"),
        "and stops on the last fact, the chip: {:?}",
        rows[last as usize]
    );
    assert!(
        rows[first as usize].contains("cache"),
        "the seam begins with the first fact: {:?}",
        rows[first as usize]
    );
    for y in first..=last {
        assert_eq!(
            buf[(left, y)].fg,
            theme::SEAM,
            "the seam is a step above faint: ({left},{y})"
        );
    }
    // Above the first fact and below the last the rail paints nothing at all.
    for y in (0..body as u16).filter(|y| *y < first || *y > last) {
        let cell = &buf[(left, y)];
        assert_eq!(cell.symbol(), " ", "nothing is painted at ({left},{y})");
        assert_eq!(
            (cell.fg, cell.bg),
            (ratatui::style::Color::Reset, ratatui::style::Color::Reset)
        );
    }
}

#[test]
fn the_seam_begins_below_the_face_that_is_drawn_above_it() {
    // The face takes the top of the rail, and the seam does not start in it:
    // the spine begins on the first fact, whatever is drawn above.
    let face = Face::kitty().expect("the embedded face decodes");
    let mut app = App::new(launch(), Some(face));
    let buf = buffer(&mut app, 100, 40);
    let rows = rails(&screen(&mut app, 100, 40), 100);
    let left = (100 - RAIL) as u16;
    let cache = rows
        .iter()
        .position(|row| row.contains("cache"))
        .expect("the rail says what the run spent");
    let first = (0..buf.area.height)
        .find(|y| buf[(left, *y)].symbol() == "⁞")
        .expect("the seam has a top");
    assert!(first > 0, "the face has the rows above it: {rows:#?}");
    assert_eq!(
        first as usize, cache,
        "the seam starts on the first fact, not on the face: {rows:#?}"
    );
}

#[test]
fn the_chip_hangs_off_the_values_edge_and_runs_to_the_rails_edge() {
    let mut app = spent();
    let buf = buffer(&mut app, 100, 26);
    let left = (100 - RAIL) as u16;
    let posture = (0..buf.area.height)
        .find(|y| (left..100).any(|x| buf[(x, *y)].symbol() == "⬢"))
        .expect("the rail wears the posture's glyph");
    let raised: Vec<u16> = (left..100)
        .filter(|x| buf[(*x, posture)].bg == theme::RAISED)
        .collect();
    assert_eq!(
        (raised.first(), raised.last()),
        (Some(&left), Some(&99)),
        "the chip's raised run begins at the seam and runs unbroken to the \
         rail's last column, the edge every value hangs off"
    );
    let chip: String = raised
        .iter()
        .map(|x| buf[(*x, posture)].symbol().to_string())
        .collect();
    assert_eq!(
        chip.chars()
            .filter(|cell| *cell != '⁞')
            .collect::<String>()
            .trim(),
        "⬢ bwrap · 1 writable",
        "the chip states the posture whole, right-aligned: {chip:?}"
    );
}
