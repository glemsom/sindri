//! The rail: every fact a user must never lose, at the widths the frame is
//! judged at, with and without the face. The rail is a ledger — a
//! dotted seam, a nine-column label field in FAINT, dot leaders across the gap
//! and the value hanging off the last column — and a fact is one row however
//! long its value is.

use super::*;

use sindri_tui::theme;

/// The labels the rail states, in the order it states them.
const LABELS: &[&str] = &[
    "cache",
    "turns",
    "tokens",
    "context",
    "model",
    "workspace",
    "branch",
    "session",
    "temp",
];

/// The rail as one line under the transcript, where the terminal gives it no
/// column of its own.
fn strip_of(app: &mut App, width: u16) -> String {
    screen(app, width, 20)
        .into_iter()
        .find(|row| row.contains("cache"))
        .expect("the strip is one line")
}

#[test]
fn the_rail_is_complete_without_the_face_at_a_hundred_and_at_a_hundred_and_sixty() {
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
            assert_eq!(
                value(&rows, label),
                said,
                "the rail states {label:?} at {width} columns: {rows:#?}"
            );
        }
        assert!(
            rows.iter()
                .any(|rail| rail.contains("⬢ bwrap · 1 writable")),
            "the posture is on the rail at {width} columns: {rows:#?}"
        );
    }
}

#[test]
fn a_guid_and_a_temp_path_are_shortened_in_the_middle_and_never_broken_across_rows() {
    // A 36-character GUID and a path longer than the rail keep both their
    // ends on one row: the head says which record, the tail says where it
    // ends, and neither is read across two rows to find out.
    for width in [100u16, 160] {
        let mut app = spent();
        let rows = rails(&screen(&mut app, width, 26), width);
        let session = fact(&rows, "session");
        assert!(session.contains('…'), "{width} columns: {session:?}");
        assert!(session.contains("4f1c8f2e"), "{width} columns: {session:?}");
        assert!(
            session.trim_end().ends_with("5c1a77"),
            "the tail of the GUID is on the same row as its head: {session:?}"
        );
        let temp = fact(&rows, "temp");
        assert!(temp.contains('…'), "{width} columns: {temp:?}");
        assert!(temp.contains("/state"), "{width} columns: {temp:?}");
        assert!(temp.contains("/tmp"), "{width} columns: {temp:?}");
        // Nothing is wrapped onto a second row: each ends the rail's one row
        // it is on and every other row is another fact.
        assert_eq!(
            rows.iter().filter(|rail| rail.contains("5c1a77")).count(),
            1,
            "{width} columns: the GUID is one row: {rows:#?}"
        );
        assert_eq!(
            rows.iter().filter(|rail| rail.contains("/tmp")).count(),
            1,
            "{width} columns: the temp path is one row: {rows:#?}"
        );
    }
}

#[test]
fn the_rail_states_the_model_and_the_workspace_and_a_resize_to_ninety_loses_neither() {
    let mut app = spent();
    let rows = rails(&screen(&mut app, 100, 26), 100);
    assert_eq!(value(&rows, "model"), "claude-sonnet-4-6");
    assert_eq!(value(&rows, "workspace"), "~/git/sindri");

    // Below a hundred columns the rail is one line under the transcript, and
    // it still says what the run talks to and what it can touch.
    let mut app = spent();
    let strip = strip_of(&mut app, 90);
    assert!(strip.contains("claude-sonnet-4-6"), "the strip: {strip:?}");
    assert!(strip.contains("~/git/sindri"), "the strip: {strip:?}");
}

#[test]
fn the_strip_carries_the_rails_facts_in_the_rails_order() {
    let mut app = spent();
    let column = rails(&screen(&mut app, 100, 26), 100);
    let mut app = spent();
    let strip = strip_of(&mut app, 90);

    // Every fact the strip has room for is one of the rail's, and it is in
    // the order the column reads them in.
    let mut cursor = 0;
    for label in LABELS {
        let Some(at) = strip.find(label) else {
            continue;
        };
        assert!(
            at >= cursor,
            "the strip reads {label:?} after the fact before it: {strip:?}"
        );
        cursor = at;
        assert_eq!(
            value(&column, label),
            value(&column, label),
            "the strip states the rail's own value for {label:?}"
        );
    }
    assert!(
        cursor > 0,
        "the strip states facts from the rail: {strip:?}"
    );
}

#[test]
fn the_sandbox_posture_is_a_chip_and_not_another_label_value_pair() {
    let mut app = spent();
    let buf = buffer(&mut app, 100, 26);
    let left = 100 - RAIL;
    let symbol = |x: u16, y: u16| buf[(x, y)].symbol().to_string();
    let posture = (0..buf.area.height)
        .find(|y| (left as u16..100).any(|x| symbol(x, *y) == "⬢"))
        .expect("the rail wears the posture's glyph");
    let raised: Vec<u16> = (left as u16..100)
        .filter(|x| buf[(*x, posture)].bg == theme::RAISED)
        .collect();
    assert!(
        !raised.is_empty(),
        "the posture sits on the tier above the panel it is drawn on"
    );
    assert_eq!(
        *raised.first().expect("a chip") + raised.len() as u16 - 1,
        *raised.last().expect("a chip"),
        "the chip is one run, not a background behind the whole row"
    );
    let chip: String = raised.iter().map(|x| symbol(*x, posture)).collect();
    assert!(
        chip.contains("bwrap · 1 writable"),
        "the chip is the posture, whole: {chip:?}"
    );
    // Nothing else on the rail is a chip: a fact is a label and a value.
    for y in 0..buf.area.height {
        if y == posture {
            continue;
        }
        for x in left as u16..100 {
            assert_ne!(
                buf[(x, y)].bg,
                theme::RAISED,
                "a plain fact is not raised: ({x},{y})"
            );
        }
    }

    // A plain fact: the label in FAINT, the value at full strength.
    let cache = (0..buf.area.height)
        .find(|y| symbol(left as u16 + 2, *y) == "c")
        .expect("the rail counts what the run spent");
    assert_eq!(
        buf[(left as u16 + 2, cache)].fg,
        theme::FAINT,
        "a label is faint"
    );
    assert_ne!(
        buf[(left as u16 + RAIL as u16 - 1, cache)].fg,
        theme::FAINT,
        "the value that hangs off the last column is at full strength"
    );
}

#[test]
fn the_rule_separates_the_numbers_from_what_the_run_is() {
    let mut app = spent();
    let rows = screen(&mut app, 100, 26);
    let at = |needle: &str| {
        rows.iter()
            .position(|row| rail_of(row).contains(needle))
            .unwrap_or_else(|| panic!("the rail says {needle:?}: {rows:#?}"))
    };
    let stop = at("stop");
    let model = at("model");
    assert_eq!(
        model,
        stop + 2,
        "the identity block is one rule away from the numbers: {rows:#?}"
    );
    assert_eq!(
        rail_of(&rows[stop + 1]),
        "─".repeat(RAIL),
        "and the rule spans the row between them: {rows:#?}"
    );
    for row in rows.iter().take(20) {
        let rail = rail_of(row);
        if rail.trim().is_empty() {
            continue;
        }
        // A row of the rail is one of four things, and each has one column it
        // may start at: blank canvas, the rule — the column's, so it spans the
        // width — the chip's raised run, right-aligned on the values' edge, and
        // everything else, which is a fact or the row that counts the lost ones
        // and starts inside the inset so no word sits against the seam.
        let text = rail.trim_start();
        let inset = rail.chars().count() - text.chars().count();
        if text.starts_with('─') {
            assert_eq!(inset, 0, "the rule spans the rail's width: {row:?}");
        } else if text.starts_with('⬢') {
            assert!(
                rail.ends_with(text),
                "the chip ends on the rail's last column: {row:?}"
            );
        } else {
            assert_eq!(
                inset,
                render::RAIL_INSET,
                "every fact starts inside the rail's inset: {row:?}"
            );
        }
    }
}

#[test]
fn the_rail_never_drops_a_fact_to_keep_the_face() {
    let face = Face::kitty().expect("the embedded face decodes");
    let mut app = App::new(launch(), Some(face));
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
    let rows = rails(&screen(&mut app, 100, 26), 100);
    for label in LABELS {
        assert!(
            !fact(&rows, label).trim().is_empty(),
            "the picture is above the facts, never instead of them: {label:?} in {rows:#?}"
        );
    }
    assert!(
        rows.iter()
            .any(|rail| rail.contains("⬢ bwrap · 1 writable")),
        "the posture and the record survive the picture: {rows:#?}"
    );
}

#[test]
fn the_facts_start_the_row_after_the_face_and_not_one_row_lower() {
    // A tall terminal, so the face draws. The picture keeps its shape: in
    // cells twice as tall as they are wide a square crop takes fewer rows
    // than the rail allows it, and reserving the difference would leave a
    // hole between Sindri's face and his facts.
    let face = Face::kitty().expect("the embedded face decodes");
    let drawn = face.rows(Rect::new(0, 0, RAIL as u16, 30));
    assert!(
        drawn < sindri_tui::face::ROWS,
        "a square picture in tall cells takes fewer rows than the rail allows: {drawn}"
    );

    let mut app = App::new(launch(), Some(face));
    let rows = screen(&mut app, 100, 40);
    let first_fact = rows
        .iter()
        .position(|row| rail_of(row).contains("cache"))
        .expect("the rail says what the run has spent");
    assert_eq!(
        first_fact as u16, drawn,
        "the first fact follows the picture directly: {rows:#?}"
    );
}

#[test]
fn the_context_meter_fills_as_the_conversation_takes_the_window() {
    for width in [100u16, 160] {
        let mut app = app();
        app.finished(Finished {
            stopped: Stop::Final,
            turns: 1,
            tokens: Tokens {
                input: 100_000,
                output: 500,
                cached: 30_000,
            },
            error: None,
            note: None,
        });
        let rows = rails(&screen(&mut app, width, 26), width);
        assert_eq!(
            value(&rows, "context"),
            "████░░░░ 100k/200k",
            "half a window reads as half a meter at {width} columns: {rows:#?}"
        );
        assert_eq!(value(&rows, "cache"), "30%", "{width} columns: {rows:#?}");
    }
}
