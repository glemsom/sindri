//! The frame seam: `ratatui`'s test backend renders the frame to a character
//! buffer, and every assertion is about *what the user sees* — a call while
//! it runs, a call once it has collapsed, the rail at 100 columns and as a
//! strip at 90, a markdown table, an error where it happened, and the card a
//! broken sandbox draws (ADR-0011).

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use sindri_core::agent::Event;
use sindri_tui::driver::{CallOver, Finished, Side, Spilled, Stop, Tokens};
use sindri_tui::render;
use sindri_tui::{App, Face, Launch};

const RAIL: usize = 32;

fn launch() -> Launch {
    Launch {
        session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
        temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
        branch: Some("t15-the-frame".to_string()),
        sandbox: "bwrap · 1 writable".to_string(),
        workspace: "~/git/sindri".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        max_turns: 250,
        context_limit: Some(200_000),
    }
}

fn app() -> App {
    App::new(launch(), None)
}

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

fn rail_of_width(row: &str, width: usize) -> String {
    row.chars().skip(width - RAIL).collect()
}

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
        note: None,
    });
    let rows = screen(&mut app, 100, 26);
    let rail: String = rows.iter().map(|row| rail_of(row)).collect();

    for fact in [
        // A grid: the label in its nine-column field, the value beside it.
        "cache     82%",
        "turns     1/250",
        "tokens    12.4k in · 2.1k out",
        "context   ░░░░░░░░ 12.4k/200k",
        // The two facts the rail used to lack, and what the run is.
        "model     claude-sonnet-4-6",
        "workspace ~/git/sindri",
        "branch    t15-the-frame",
        // A GUID and a temp path longer than the rail keep both their ends
        // on one row: the record is always findable, never half an id.
        "4f1c8f2e-8…",
        "f0e5c1a77",
        "/state/sin…",
        "c8f2e/tmp",
        // And the posture is a chip of its own, not another pair.
        "⬢ bwrap · 1 writable",
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
        note: None,
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
fn a_record_that_could_not_be_closed_is_said_in_the_transcript() {
    let mut app = app();
    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        note: Some(
            "session 4f1c8f2e: the record could not be finished: no space left on device"
                .to_string(),
        ),
        ..Finished::default()
    });
    let rows = screen(&mut app, 100, 24);
    assert!(
        any(&rows, "the record could not be finished"),
        "a record the run could not close is said out loud, never dropped: {rows:#?}"
    );
    // The note wraps where its column ends, so its reason is read across the
    // wrap as well — row by row, in order, and each row keeps its marker.
    let reason: Vec<&String> = rows.iter().filter(|row| row.starts_with('!')).collect();
    let reason = reason
        .iter()
        .map(|row| row.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        reason.contains("no space") && reason.contains("left on device"),
        "and the reason is the record's own, wrapped and all: {reason:#?}"
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

mod card;
mod prose;
mod queue;
mod rail;
mod surface;
mod transcript;
