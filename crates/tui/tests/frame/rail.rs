//! The rail: every fact a user must never lose, at the widths the frame is
//! judged at, with and without the face (ADR-0011 §4, ADR-0014).

use super::*;

/// Every fact the rail must never lose, as the text a user reads.
const FACTS: &[&str] = &[
    "cache",
    "turns 1/250",
    "tokens",
    "context",
    "200k",
    "session",
    "temp",
    "branch",
    "t15-the-frame",
    "sandbox",
    "bwrap · 1 writable",
];

/// A frame with one finished run on it: what a rail has to say after a prompt.
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
    });
    app
}

#[test]
fn the_rail_is_complete_without_the_face_at_a_hundred_and_at_a_hundred_and_sixty() {
    for width in [100u16, 160] {
        let mut app = spent();
        let rows = screen(&mut app, width, 26);
        let rail: String = rows
            .iter()
            .map(|row| rail_of_width(row, width as usize))
            .collect::<Vec<_>>()
            .join("\n");
        for fact in FACTS {
            assert!(
                rail.contains(fact),
                "{width} columns shows {fact:?}: {rail}"
            );
        }
        // The meter is a percentage of the window, and the cache a share of the
        // input: both are numbers, not placeholders.
        assert!(rail.contains("cache 82%"), "{width} columns: {rail}");
        assert!(
            rail.contains("░░░░░░░░ 12.4k/200k"),
            "the meter reads its window at {width} columns: {rail}"
        );
    }
}

#[test]
fn the_rail_never_drops_a_fact_to_keep_the_face() {
    // A short terminal: the picture fits only by pushing the sandbox posture
    // off the screen, so it does not draw at all.
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
    });
    let rows = screen(&mut app, 100, 26);
    let rail: String = rows
        .iter()
        .map(|row| rail_of(row))
        .collect::<Vec<_>>()
        .join("\n");
    for fact in FACTS {
        assert!(
            rail.contains(fact),
            "the posture and the record survive the picture: {fact:?} in {rail}"
        );
    }
}

#[test]
fn the_rule_down_the_rail_runs_unbroken_between_the_facts() {
    let mut app = spent();
    let rows = screen(&mut app, 100, 26);
    let at = |needle: &str| {
        rows.iter()
            .position(|row| rail_of(row).contains(needle))
            .unwrap_or_else(|| panic!("the rail says {needle:?}: {rows:#?}"))
    };
    let stop = at("stop final");
    let session = at("session");
    assert_eq!(session, stop + 2, "the session is one blank row apart");
    assert_eq!(
        rail_of(&rows[stop + 1]).trim_end(),
        "│",
        "and the rule runs through the blank row between them: {rows:#?}"
    );
    // No row of the rail is a gap in the line: a break reads as a broken
    // panel, not as a grouping.
    for row in rows.iter().take(20) {
        let rail = rail_of(row);
        assert!(
            rail.trim().is_empty() || rail.starts_with('│'),
            "every row of the rail carries its rule: {row:?}"
        );
    }
}

#[test]
fn the_context_meter_fills_as_the_conversation_takes_the_window() {
    // Half of a 200k window: four of eight cells, at both widths the rail is
    // judged at. A meter that never fills is a meter nobody can read.
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
        });
        let rows = screen(&mut app, width, 26);
        let rail: String = rows
            .iter()
            .map(|row| rail_of_width(row, width as usize))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            rail.contains("████░░░░ 100k/200k"),
            "half a window reads as half a meter at {width} columns: {rail}"
        );
        assert!(rail.contains("cache 30%"), "{width} columns: {rail}");
    }
}
