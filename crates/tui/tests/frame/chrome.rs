//! The chrome's budget: the footer never cuts a hint in half, the rail never
//! loses the posture, and a fact too long for its room is shortened rather
//! than skipped.

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;

fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

/// Every hint the footer can state, most important first. Written out here
/// rather than read out of the renderer: a hint the frame gained and this list
/// forgot is a failure.
const HINTS: &[&str] = &[
    "Enter send",
    "? keys",
    "Ctrl-Q quit",
    "@ paths",
    "/ commands",
    "Shift+Enter newline",
    "PgUp/PgDn scroll",
];

/// The footer as the hints it is made of: a row that splits into hints this
/// list knows is a row where nothing was cut mid-word.
fn hints(rows: &[String]) -> Vec<String> {
    let footer = rows.last().expect("the frame has a footer");
    footer
        .trim_end()
        .split(" · ")
        .map(|hint| hint.to_string())
        .collect()
}

/// The strip: the one rail row under the transcript, a row above the composer.
fn strip(rows: &[String]) -> String {
    let composer = rows
        .iter()
        .position(|row| row.contains("╭ prompt"))
        .expect("the composer draws");
    rows[composer - 1].trim_end().to_string()
}

/// The rail's own columns of a 100-column frame.
fn rails(rows: &[String]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.chars().count() > RAIL)
        .map(|row| row.chars().skip(row.chars().count() - RAIL).collect())
        .collect()
}

#[test]
fn the_footer_states_whole_hints_and_never_a_hint_cut_in_half() {
    for width in [70u16, 80, 100, 120, 200] {
        let mut app = app();
        let rows = screen(&mut app, width, 20);
        let shown = hints(&rows);
        for hint in &shown {
            assert!(
                HINTS.contains(&hint.as_str()),
                "{width} columns: {hint:?} is not a whole hint: {rows:#?}"
            );
        }
        assert!(
            shown.first().is_some_and(|hint| hint == "Enter send"),
            "{width} columns: the key that sends leads the line: {rows:#?}"
        );
    }
}

#[test]
fn the_footer_says_more_keys_the_more_room_it_has() {
    for (width, wanted) in [
        (40u16, 2usize),
        (60, 3),
        (70, 3),
        (80, 3),
        (100, 5),
        (120, HINTS.len()),
        (200, HINTS.len()),
    ] {
        let mut app = app();
        let rows = screen(&mut app, width, 20);
        assert_eq!(
            hints(&rows),
            HINTS[..wanted].to_vec(),
            "{width} columns states the {wanted} hints that matter most: {rows:#?}"
        );
    }
}

#[test]
fn the_keys_that_open_the_list_and_leave_are_never_the_ones_dropped() {
    for width in [60u16, 70, 80, 100, 120, 200] {
        let mut app = app();
        let rows = screen(&mut app, width, 20);
        let shown = hints(&rows);
        for wanted in ["? keys", "Ctrl-Q quit"] {
            assert!(
                shown.iter().any(|hint| hint == wanted),
                "{width} columns still says {wanted:?}: {rows:#?}"
            );
        }
    }
}

#[test]
fn the_hint_line_is_still_whole_while_a_run_is_live_and_while_one_is_cancelled() {
    for (width, prefix, cancelled) in [(70u16, "working", false), (70, "cancelling…", true)] {
        let mut app = app();
        for ch in "count the files".chars() {
            app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.start().expect("a run starts");
        if cancelled {
            app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        }
        let rows = screen(&mut app, width, 20);
        let footer = rows.last().expect("a footer").trim_end();
        assert!(
            footer.starts_with(prefix),
            "{width} columns says {prefix:?} first: {rows:#?}"
        );
        for hint in footer.split(" · ").skip(1) {
            assert!(
                HINTS.contains(&hint),
                "{width} columns: {hint:?} is not a whole hint beside {prefix:?}: {rows:#?}"
            );
        }
    }
}

#[test]
fn the_strip_keeps_the_posture_and_drops_the_branch_at_forty_columns() {
    let mut app = app();
    let rows = screen(&mut app, 40, 20);
    let strip = strip(&rows);
    assert!(
        strip.contains("⬢ bwrap"),
        "the sandbox survives the narrowest frame: {strip:?}"
    );
    assert!(
        !rows.iter().any(|row| row.contains("branch t15")),
        "the branch is one of the facts the glove can lose: {rows:#?}"
    );
    assert!(
        strip.chars().count() <= 40,
        "the strip is one row of forty columns: {strip:?}"
    );
}

#[test]
fn a_rail_ten_rows_tall_keeps_the_posture_and_the_context_meter() {
    let mut app = app();
    let rows = screen(&mut app, 100, 10);
    let rail = rails(&rows);
    assert!(
        rail.iter().any(|row| row.contains("context")),
        "the meter is one of the two facts a short rail keeps: {rail:#?}"
    );
    assert!(
        rail.iter().any(|row| row.contains("⬢ bwrap")),
        "the posture is the other: {rail:#?}"
    );
    // What it could not state it says in one row, rather than by silence.
    let said = rail
        .iter()
        .find(|row| row.contains("more"))
        .unwrap_or_else(|| panic!("the short rail says what it left out: {rail:#?}"));
    let count: usize = said
        .trim()
        .trim_start_matches([' ', '+'])
        .split(' ')
        .next()
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("the row names a count: {said:?}"));
    assert!(count > 0, "and says how much: {said:?}");
    assert!(
        !rail.iter().any(|row| row.contains("temp")),
        "the facts it left out are not on the rail: {rail:#?}"
    );
}

#[test]
fn a_rail_with_the_room_for_its_facts_leaves_none_of_them_out() {
    // Twenty rows of terminal is a rail with room for every fact: nothing is
    // dropped, so nothing has to be explained away either.
    let mut app = app();
    let rows = screen(&mut app, 100, 20);
    let rail = rails(&rows);
    for label in [
        "cache",
        "turns",
        "tokens",
        "context",
        "model",
        "workspace",
        "branch",
        "session",
        "temp",
    ] {
        assert!(
            rail.iter().any(|row| row.contains(label)),
            "the rail states {label:?} when it has the room: {rail:#?}"
        );
    }
    assert!(
        !rail.iter().any(|row| row.contains(" more")),
        "and says nothing about facts it did not lose: {rail:#?}"
    );
}

#[test]
fn the_chrome_holds_at_forty_seventy_a_hundred_a_hundred_and_twenty_and_two_hundred() {
    for width in [40u16, 70, 100, 120, 200] {
        let mut app = app();
        let buf = buffer(&mut app, width, 24);
        let rows: Vec<String> = (0..24)
            .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect();
        // Not one hint is cut, whatever the width.
        for hint in hints(&rows) {
            assert!(
                HINTS.contains(&hint.as_str()),
                "{width} columns: {hint:?} is not a whole hint: {rows:#?}"
            );
        }
        // The posture is on every frame, at every width.
        assert!(
            rows.iter().any(|row| row.contains("⬢ bwrap")),
            "{width} columns says which sandbox the run is in: {rows:#?}"
        );
        // And a fact that does not fit whole is shortened, not dropped.
        if width >= 70 {
            assert!(
                rows.iter().any(|row| row.contains("claude-sonnet-4-6")),
                "{width} columns keeps the model: {rows:#?}"
            );
            assert!(
                rows.iter().any(|row| row.contains("~/git/sindri")),
                "{width} columns keeps the workspace: {rows:#?}"
            );
        }
    }
}
