//! The frame seam: `ratatui`'s test backend renders the frame to a character
//! buffer, and every assertion is about *what the user sees* — a call while it
//! runs, a call once it has collapsed, the rail at 100 columns and as a strip at
//! 90, a markdown table, an error where it happened, and the card a broken
//! sandbox draws (ADR-0011).

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sindri_core::agent::Event;
use sindri_tui::driver::{CallOver, Finished, Side, Spilled, Stop, Tokens};
use sindri_tui::render;
use sindri_tui::{App, Face, Launch};

/// The rail's width, and the width the frame gives it up: below 100 columns it
/// is a strip (ADR-0011 §4).
const RAIL: usize = 32;

fn launch() -> Launch {
    Launch {
        session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
        temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
        branch: Some("t15-the-frame".to_string()),
        sandbox: "bwrap · 1 writable".to_string(),
        max_turns: 250,
        context_limit: Some(200_000),
    }
}

fn app() -> App {
    App::new(launch(), None)
}

/// Every row of the frame, as the characters a terminal would show.
fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// The rail's own columns of a row, at a frame of `width` columns.
fn rail_of_width(row: &str, width: usize) -> String {
    row.chars().skip(width - RAIL).collect()
}

/// The rail's own columns of a row of a 100-column frame.
fn rail_of(row: &str) -> String {
    rail_of_width(row, 100)
}

fn any(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

#[test]
fn a_running_call_shows_one_line_and_the_last_five_lines_of_its_output() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("cargo test 2>&1 | tail -20".to_string()),
    });
    for line in 1..=7 {
        app.output(Side::Stdout, &format!("out {line}"));
    }
    let rows = screen(&mut app, 100, 20);

    assert!(
        any(&rows, "cargo test 2>&1 | tail -20"),
        "the script is on one line: {rows:#?}"
    );
    assert!(any(&rows, "out 7"), "the newest line is shown");
    assert!(any(&rows, "out 3"), "the last five lines are shown");
    assert!(
        !any(&rows, "out 2") && !any(&rows, "out 1"),
        "the tail is the last five, not all of them: {rows:#?}"
    );
    assert!(
        !any(&rows, "exit 0"),
        "a running call has no exit status yet: {rows:#?}"
    );
}

#[test]
fn a_collapsed_call_shows_its_exit_status_and_its_tail_is_gone() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("cargo test".to_string()),
    });
    app.output(Side::Stdout, "running 12 tests");
    app.over(CallOver::Exited(0), Spilled::default());
    let rows = screen(&mut app, 100, 20);
    assert!(
        any(&rows, "✓ exit 0"),
        "a call that worked says so: {rows:#?}"
    );
    assert!(
        !any(&rows, "running 12 tests"),
        "the tail is folded away: {rows:#?}"
    );

    let mut app = self::app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("cargo test".to_string()),
    });
    app.over(CallOver::Exited(3), Spilled::default());
    let rows = screen(&mut app, 100, 20);
    assert!(any(&rows, "✗ exit 3"), "a failed call says so: {rows:#?}");
}

#[test]
fn the_rail_is_a_column_at_a_hundred_columns() {
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
    let rows = screen(&mut app, 100, 26);
    let rail: String = rows.iter().map(|row| rail_of(row)).collect();

    for fact in [
        "cache 82%",
        "turns 1/250",
        "12.4k",
        "context",
        "200k",
        // A 36-character GUID takes two rows of a 32-column rail, head and
        // tail both on screen: the record is always findable.
        "4f1c8f2e-8a24",
        "5c1a77",
        "/state/sindri/sessions/4f1c8f2",
        "t15-the-frame",
        "bwrap · 1 writable",
    ] {
        assert!(rail.contains(fact), "the rail says {fact:?}: {rail:#?}");
    }
    assert!(
        rows.iter().any(|row| row.contains("Two files changed.")),
        "the answer has the transcript column to itself: {rows:#?}"
    );
}

#[test]
fn the_rail_collapses_to_a_one_line_strip_below_a_hundred_columns() {
    let mut app = app();
    let rows = screen(&mut app, 90, 20);
    let strips: Vec<&String> = rows.iter().filter(|row| row.contains("cache")).collect();
    assert_eq!(
        strips.len(),
        1,
        "the strip is one line, not a column: {rows:#?}"
    );
    let strip = strips[0];
    assert!(strip.contains("turns 0/250"), "{strip:?}");
    assert!(
        strip.contains("bwrap"),
        "the posture never hides: {strip:?}"
    );
    assert!(
        strip.starts_with("cache"),
        "the strip starts at the left edge: {strip:?}"
    );
}

#[test]
fn an_answer_renders_its_markdown_table_in_the_transcript() {
    let mut app = app();
    let answer = "Which files changed:\n\n| file | change |\n| --- | --- |\n| src/lib.rs | +12 |\n";
    app.event(Event::TextDelta {
        turn: 1,
        text: answer.to_string(),
    });
    let rows = screen(&mut app, 100, 24);
    assert!(
        rows.iter()
            .any(|row| row.contains("file") && row.contains("change")),
        "the header row holds both its cells: {rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("src/lib.rs") && row.contains("+12")),
        "a row holds its whole line, aligned: {rows:#?}"
    );
}

#[test]
fn a_provider_error_appears_in_the_transcript_where_it_happened() {
    let mut app = app();
    app.event(Event::TextDelta {
        turn: 1,
        text: "Looking at the test.".to_string(),
    });
    app.finished(Finished {
        stopped: Stop::Error,
        turns: 1,
        tokens: Tokens::default(),
        error: Some("provider error (HTTP 529): overloaded".to_string()),
    });
    let rows = screen(&mut app, 100, 24);
    assert!(
        any(&rows, "provider error (HTTP 529): overloaded"),
        "the failure is content, not a toast: {rows:#?}"
    );
    let rail: String = rows.iter().map(|row| rail_of(row)).collect();
    assert!(
        rail.contains("error"),
        "the rail names the stop reason: {rail:#?}"
    );
}

#[test]
fn without_kitty_graphics_the_rail_closes_up_and_starts_at_the_cache_meter() {
    let mut app = app();
    let rows = screen(&mut app, 100, 26);
    let first = rail_of(&rows[0]);
    assert!(
        first.contains("cache"),
        "no placeholder, no gap: the rail begins where the image would: {rows:#?}"
    );
}

#[test]
fn with_kitty_graphics_the_face_draws_in_the_top_of_the_rail() {
    let face = Face::kitty().expect("the embedded face decodes");
    let mut app = App::new(launch(), Some(face));
    // Tall enough for the face and every fact: the picture is the ornament, and
    // a frame that cannot hold both gives its room to the facts.
    let rows = screen(&mut app, 100, 40);
    let top = rail_of(&rows[0]);
    assert!(
        !top.contains("cache"),
        "the image has the top of the rail: {top:?}"
    );
    let rail: String = rows.iter().map(|row| rail_of(row)).collect();
    assert!(
        rail.contains("cache"),
        "and the rail still says everything, below it: {rail:#?}"
    );
}

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
