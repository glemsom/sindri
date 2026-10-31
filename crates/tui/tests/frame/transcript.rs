//! The transcript as a document: two rows between turns and none inside one,
//! a two-column gutter a call hangs off, a header that names the human, and
//! the prompt at full strength on its own band.

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use sindri_tui::render::transcript;
use sindri_tui::theme;

/// The transcript column of a 100-wide frame: the rail and the margin between
/// them take the rest.
const COLUMN: u16 = MARGIN as u16;

/// A row of the transcript column with nothing on it. The rail keeps its own
/// column on every row, so only the column the transcript has is read.
fn blank(row: &str) -> bool {
    row.chars().take(COLUMN as usize).all(|ch| ch == ' ')
}

fn column_of(rows: &[String], row: usize, needle: &str) -> usize {
    rows[row]
        .find(needle)
        .unwrap_or_else(|| panic!("row {row} does not say {needle:?}: {rows:#?}"))
}

#[test]
fn two_turns_are_two_rows_apart_and_one_turn_is_no_rows_apart() {
    let mut app = app();
    turn(&mut app, "list the files", 1, "ls -la");
    turn(&mut app, "and now the diff", 2, "git diff");
    let rows = screen(&mut app, 100, 30);

    let first = row_of(&rows, "YOU");
    let second = first + 1 + row_of(&rows[first + 1..], "YOU");
    assert!(
        rows[second].contains("~/git/sindri"),
        "a turn header names the human, the hour and the directory: {rows:#?}"
    );

    // Two rows separate the turns, and they are the only two: everything
    // above them belongs to the first turn.
    assert!(
        blank(&rows[second - 1]) && blank(&rows[second - 2]),
        "two rows separate the turns: {rows:#?}"
    );
    assert!(
        !blank(&rows[second - 3]),
        "the turn before is two rows above the header, not three: {rows:#?}"
    );
    assert!(
        (first..second - 2).all(|row| !blank(&rows[row])),
        "nothing inside a turn is a blank row: {rows:#?}"
    );

    // A call and its own output are one block: no row between them, either.
    let call = row_of(&rows, "⎿");
    assert!(
        rows[call + 1].contains("out 1"),
        "a call's output hangs off it with no blank row between: {rows:#?}"
    );
}

#[test]
fn a_call_hangs_off_the_gutter_its_answer_starts_at() {
    let mut app = app();
    turn(&mut app, "run the tests", 1, "cargo test");
    let rows = screen(&mut app, 100, 30);

    let answer = row_of(&rows, "answer 1");
    let call = row_of(&rows, "⎿");
    let prompt = row_of(&rows, "run the tests");

    assert_eq!(
        column_of(&rows, answer, "answer"),
        2,
        "prose sits two columns in from the turn's edge: {rows:#?}"
    );
    assert_eq!(
        column_of(&rows, call, "⎿"),
        2,
        "a call's first row is indented under its answer's gutter: {rows:#?}"
    );
    assert!(
        column_of(&rows, call, "cargo test") > 2,
        "and the call itself hangs off that gutter: {rows:#?}"
    );
    assert_eq!(
        column_of(&rows, prompt, "› "),
        0,
        "the band starts at the turn's edge, its marker in the first column: {rows:#?}"
    );
}

#[test]
fn a_running_call_and_the_same_call_collapsed_agree_on_every_column() {
    let mut app = app();
    type_text(&mut app, "build it");
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.start().expect("a prompt starts a run");
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("cargo build".to_string()),
    });
    let running = screen(&mut app, 100, 20);
    let live = row_of(&running, "cargo build");
    let live_command = column_of(&running, live, "cargo build");
    let live_status = column_of(&running, live, "running");

    app.over(CallOver::Exited(0), Spilled::default());
    let collapsed = screen(&mut app, 100, 20);
    let done = row_of(&collapsed, "cargo build");
    let done_command = column_of(&collapsed, done, "cargo build");
    let done_status = column_of(&collapsed, done, "exit 0");

    assert_eq!(
        live_command, done_command,
        "the command does not move when the call ends: {running:#?} {collapsed:#?}"
    );
    assert_eq!(
        live_status, done_status,
        "and neither does the status it is read with: {running:#?} {collapsed:#?}"
    );
    assert!(
        collapsed[done].contains('✓'),
        "a finished call carries the status glyph: {collapsed:#?}"
    );
}

#[test]
fn the_prompt_is_full_strength_on_a_panel_band_under_a_header() {
    let mut app = app();
    turn(&mut app, "why is the build slow", 1, "make");
    let buf = buffer(&mut app, 100, 24);
    let rows = screen(&mut app, 100, 24);

    let header = row_of(&rows, "YOU");
    assert_eq!(
        buf[(0, header as u16)].fg,
        theme::PROMPT,
        "the header opens with the human, in the frame's own accent"
    );
    let header: String = rows[header].chars().take(COLUMN as usize).collect();
    let mut fields = header.split_whitespace();
    assert_eq!(fields.next(), Some("YOU"), "{rows:#?}");
    let at = fields.next().unwrap_or_default();
    assert_eq!(
        (at.len(), at.chars().nth(2)),
        (5, Some(':')),
        "the header carries the hour a turn was asked at: {header:?}"
    );
    assert_eq!(
        fields.next(),
        Some("~/git/sindri"),
        "and the directory it was asked from: {header:?}"
    );

    let band = row_of(&rows, "why is the build slow") as u16;
    for x in 0..COLUMN {
        assert_eq!(
            buf[(x, band)].bg,
            theme::PANEL,
            "the prompt is a band across the whole column at ({x},{band}): {rows:#?}"
        );
    }
    let middle = COLUMN / 2;
    assert_eq!(
        buf[(middle, band)].fg,
        theme::PLAIN,
        "the prompt is prose and not chrome, and prose has an ink: the theme's, \
         never the terminal's default foreground"
    );
    assert_eq!(
        buf[(0, band)].fg,
        theme::PROMPT,
        "with a marker in the prompt's own colour"
    );

    let answer = row_of(&rows, "answer 1") as u16;
    assert_eq!(
        buf[(middle, answer)].bg,
        theme::CANVAS,
        "an answer sits on the canvas the theme names, not on the prompt's band"
    );
}

#[test]
fn a_five_hundred_entry_thread_still_lays_out_only_a_screenful() {
    let mut app = app();
    for turn in 1..=500 {
        app.note_info(format!("question {turn}"));
        app.event(Event::TextDelta {
            turn,
            text: format!("answer {turn}"),
        });
    }
    let view = transcript::window(&mut app, 80, 20);
    assert_eq!(view.rows.len(), 20, "a window is a screen tall");
    assert!(
        view.laid_out <= 26,
        "the spacing rules do not put the thread in the render tree: laid out {} entries",
        view.laid_out
    );
    assert!(
        view.rows
            .iter()
            .any(|row| transcript::plain(row).contains("answer 500")),
        "and it is still anchored at the newest row"
    );
}
