//! The rail reads as a ledger: values hanging off one edge, a dot leader
//! bridging the label's field to the figure where there is a gap to bridge, a
//! hairline between the blocks, and a dotted seam that begins and ends with
//! the facts.

use super::*;

use ratatui::buffer::Buffer;
use sindri_tui::theme;

/// The nine columns a label sits in, after the rail's inset.
const FIELD: std::ops::Range<usize> = render::RAIL_INSET..render::RAIL_INSET + LABEL;
/// Where the room for a gap and a value starts: the label's field and the
/// space after it.
const LEDGER: usize = render::RAIL_INSET + LABEL + 1;

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
                format!("{label:<LABEL$}"),
                "{width} columns: {label:?} keeps the nine-column field: {row:?}"
            );
            assert_eq!(
                cells[render::RAIL_INSET + LABEL],
                ' ',
                "{width} columns: {label:?}: {row:?}"
            );
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
    // A value that fills its room has no gap left to bridge: `session`'s
    // shortened GUID is the ledger's twenty cells whole, so it draws no dots at
    // all.
    let guid = fact(&rows, "session");
    assert_eq!(
        guid.chars().count(),
        RAIL,
        "the id hangs off the rail's last column: {guid:?}"
    );
    assert!(
        !guid.contains('·'),
        "a leader with nothing to bridge is noise: {guid:?}"
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
        .find(|y| buf[(left, *y)].symbol() == "─")
        .expect("the identity block opens with a rule");
    let rule: String = (left..100)
        .map(|x| buf[(x, hairline)].symbol().to_string())
        .collect();
    assert_eq!(
        rule,
        "─".repeat(RAIL),
        "the rule spans the rail's whole width: {rule:?}"
    );
    for x in left..100 {
        assert_eq!(
            buf[(x, hairline)].fg,
            theme::FAINT,
            "the hairline is faint: ({x},{hairline})"
        );
    }
    // It groups the blocks rather than dividing the column from the page: the
    // numbers above it, what the run is below it.
    let rows = rails(&screen(&mut app, 100, 26), 100);
    // The rows either side of the rule are facts like any other, so they start
    // where every fact starts: inside the rail's inset.
    let opens = |at: usize| -> String {
        rows[at]
            .chars()
            .skip(render::RAIL_INSET)
            .take(5)
            .collect::<String>()
    };
    assert!(
        opens(hairline as usize - 1).starts_with("stop"),
        "the rule closes the numbers: {:?}",
        rows[hairline as usize - 1]
    );
    assert!(
        opens(hairline as usize + 1).starts_with("model"),
        "and opens what the run is: {:?}",
        rows[hairline as usize + 1]
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
        "the chip's raised run begins at the rail's first column and runs \
         unbroken to its last, the edge every value hangs off"
    );
    let chip: String = raised
        .iter()
        .map(|x| buf[(*x, posture)].symbol().to_string())
        .collect();
    assert_eq!(
        chip.trim(),
        "⬢ bwrap · 1 writable",
        "the chip states the posture whole, right-aligned: {chip:?}"
    );
}
