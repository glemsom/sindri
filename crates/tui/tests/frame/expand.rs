//! Every call is openable, reasoning included: a call keeps
//! a bounded tail of its whole output, `Enter` on a blank composer — or a click
//! on its row — expands it in place, and the spill file is an optimisation on
//! top of that rather than the way back in.

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use sindri_tui::entry::{Call, Entry, FULL_LINES};

/// A call that ran `lines` lines of output and exited 0, with no spill.
fn call_with(app: &mut App, turn: u32, command: &str, lines: usize) {
    app.event(Event::ToolCall {
        turn,
        command: Some(command.to_string()),
    });
    for line in 1..=lines {
        app.output(Side::Stdout, &format!("line {line}"));
    }
    app.over(CallOver::Exited(0), Spilled::default());
}

fn enter(app: &mut App) {
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

fn page_down(app: &mut App) {
    app.key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
}

fn click(app: &mut App, row: usize) {
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 4,
        row: row as u16,
        modifiers: KeyModifiers::NONE,
    });
}

fn calls(app: &App) -> Vec<&Call> {
    app.entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Call(call) => Some(call),
            _ => None,
        })
        .collect()
}

#[test]
fn a_four_hundred_line_call_expands_in_place_and_collapses_again() {
    let mut app = app();
    call_with(&mut app, 1, "cargo test", 400);
    let collapsed = screen(&mut app, 100, 30);
    assert!(
        any(&collapsed, "400 lines"),
        "a collapsed call says what is behind it: {collapsed:#?}"
    );
    assert!(
        !any(&collapsed, "line 400"),
        "and holds it folded away: {collapsed:#?}"
    );

    enter(&mut app);
    let expanded = screen(&mut app, 100, 30);
    assert!(
        any(&expanded, "line 1"),
        "one key opens the newest call on its own row, with its output beneath: {expanded:#?}"
    );
    assert!(
        !any(&expanded, "line 400"),
        "and the window is a screen tall, not the whole output: {expanded:#?}"
    );

    enter(&mut app);
    let folded = screen(&mut app, 100, 30);
    assert!(
        !any(&folded, "line 1"),
        "the same key folds it again: {folded:#?}"
    );
    assert!(
        any(&folded, "cargo test"),
        "and the call's own row is still there: {folded:#?}"
    );
}

#[test]
fn the_expansion_scrolls_within_the_window() {
    let mut app = app();
    call_with(&mut app, 1, "cargo test", 400);
    // Opening holds the window on the call's own row, so the body is read
    // forwards from the first line the call kept.
    enter(&mut app);
    let top = screen(&mut app, 100, 30);
    assert!(
        any(&top, "line 1"),
        "the first row of the expansion is under the call's own: {top:#?}"
    );

    page_down(&mut app);
    let later = screen(&mut app, 100, 30);
    assert_ne!(later, top, "PgDn moves inside the expansion");
    assert!(
        !any(&later, "line 1"),
        "the rows a screen on are shown instead: {later:#?}"
    );

    for _ in 0..40 {
        page_down(&mut app);
    }
    let bottom = screen(&mut app, 100, 30);
    assert!(
        any(&bottom, "line 400"),
        "scrolling reaches the last line the call kept: {bottom:#?}"
    );
}

#[test]
fn a_collapsed_call_says_its_status_its_duration_and_its_line_count() {
    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("make test".to_string()),
    });
    for line in 1..=14 {
        app.output(Side::Stdout, &format!("out {line}"));
    }
    app.over(CallOver::Exited(0), Spilled::default());
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .find(|row| row.contains("make test"))
        .unwrap_or_else(|| panic!("no call row: {rows:#?}"));

    for fact in ["✓ exit 0", "0.0s", "14 lines"] {
        assert!(row.contains(fact), "the call row says {fact:?}: {row:?}");
    }
    assert!(
        row.find("14 lines").unwrap_or(usize::MAX) < row.find("make test").unwrap_or(0),
        "the count sits between the duration and the command: {row:?}"
    );
}

#[test]
fn a_single_line_says_one_line_not_one_lines() {
    let mut app = app();
    call_with(&mut app, 1, "ls", 1);
    let rows = screen(&mut app, 100, 20);
    assert!(any(&rows, "1 line"), "{rows:#?}");
    assert!(!any(&rows, "1 lines"), "{rows:#?}");
}

#[test]
fn reasoning_expands_and_collapses_and_its_marker_stays_one_row() {
    let mut app = app();
    app.event(Event::Reasoning {
        turn: 1,
        text: "First I read the failing test.\nThen I changed the parser.".to_string(),
    });
    let collapsed = screen(&mut app, 100, 20);
    let markers = collapsed
        .iter()
        .filter(|row| row.contains('⌁'))
        .collect::<Vec<_>>();
    assert_eq!(
        markers.len(),
        1,
        "the collapsed marker keeps its one-row form: {collapsed:#?}"
    );
    assert!(
        !any(&collapsed, "Then I changed the parser"),
        "and the thinking is folded away: {collapsed:#?}"
    );

    enter(&mut app);
    let expanded = screen(&mut app, 100, 20);
    assert!(
        any(&expanded, "First I read the failing test"),
        "the same key opens the reasoning: {expanded:#?}"
    );
    assert!(
        any(&expanded, "Then I changed the parser"),
        "to the text it held: {expanded:#?}"
    );

    enter(&mut app);
    let folded = screen(&mut app, 100, 20);
    assert!(
        !any(&folded, "First I read the failing test"),
        "and folds it again: {folded:#?}"
    );
    assert_eq!(
        folded.iter().filter(|row| row.contains('⌁')).count(),
        1,
        "one row, as it was: {folded:#?}"
    );
}

#[test]
fn a_call_that_produced_no_spill_is_still_expandable() {
    let mut app = app();
    call_with(&mut app, 1, "cargo test", 3);
    assert!(
        app.entries().iter().any(|entry| match entry {
            Entry::Call(call) => call.spill_file().is_none() && call.expandable(),
            _ => false,
        }),
        "the spill is an optimisation, not the feature"
    );
    enter(&mut app);
    let rows = screen(&mut app, 100, 20);
    for line in 1..=3 {
        assert!(any(&rows, &format!("line {line}")), "{rows:#?}");
    }
}

#[test]
fn a_click_on_a_call_row_opens_it_and_a_second_click_folds_it() {
    let mut app = app();
    call_with(&mut app, 1, "cargo test", 10);
    let rows = screen(&mut app, 100, 30);
    let row = rows
        .iter()
        .position(|row| row.contains("exit 0"))
        .expect("the call's own row is on screen");

    click(&mut app, row);
    let expanded = screen(&mut app, 100, 30);
    assert!(
        any(&expanded, "line 10"),
        "a click on the row expands that call: {expanded:#?}"
    );
    let open = expanded
        .iter()
        .position(|row| row.contains("exit 0"))
        .expect("and its row is still where it was");
    assert!(
        expanded[open].contains('▾'),
        "the row says it is open: {expanded:#?}"
    );

    click(&mut app, open);
    let folded = screen(&mut app, 100, 30);
    assert!(
        !any(&folded, "line 10"),
        "and a second click folds it: {folded:#?}"
    );
}

#[test]
fn a_click_opens_the_call_it_was_on_and_not_the_newest() {
    let mut app = app();
    call_with(&mut app, 1, "first", 20);
    call_with(&mut app, 2, "second", 20);
    let rows = screen(&mut app, 100, 30);
    let older = rows
        .iter()
        .position(|row| row.contains("exit 0"))
        .expect("the older call is on screen");

    click(&mut app, older);
    let expanded = screen(&mut app, 100, 30);
    let opened = calls(&app)
        .into_iter()
        .filter(|call| call.expanded())
        .count();
    assert_eq!(opened, 1, "one call opens, not both: {expanded:#?}");
    assert!(
        calls(&app)[0].expanded(),
        "the click opened the row it was on: {expanded:#?}"
    );
}

#[test]
fn ctrl_o_still_opens_the_untruncated_stream_and_nothing_alarming_without_one() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    let full: String = (0..500).map(|line| format!("full line {line}\n")).collect();
    std::fs::write(&path, full).expect("write the spill file");

    let mut app = app();
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("seq 500".to_string()),
    });
    app.over(
        CallOver::Exited(0),
        Spilled {
            truncated: true,
            files: vec![path],
        },
    );
    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.overlay().is_some(), "Ctrl-O opens the whole stream");
    app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    let mut quiet = self::app();
    call_with(&mut quiet, 1, "echo hi", 2);
    quiet.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(
        quiet.overlay().is_none(),
        "a call with no spill opens no box: the expansion is the way in"
    );
    let rows = screen(&mut quiet, 100, 20);
    assert!(
        any(&rows, "no output in this session was too large to show"),
        "and the frame says why nothing happened: {rows:#?}"
    );
}

#[test]
fn five_hundred_calls_keep_a_bounded_tail_each() {
    let mut app = app();
    for turn in 1..=500 {
        app.event(Event::ToolCall {
            turn,
            command: Some(format!("run {turn}")),
        });
        for line in 1..=1000 {
            app.output(Side::Stdout, &format!("turn {turn} line {line}"));
        }
        app.over(CallOver::Exited(0), Spilled::default());
    }
    let calls = calls(&app);
    assert_eq!(calls.len(), 500);
    for call in &calls {
        assert!(
            call.kept().len() <= FULL_LINES,
            "an entry holds a bounded tail, not its whole output: kept {}",
            call.kept().len()
        );
        assert_eq!(
            call.lines(),
            1000,
            "and still counts every line it was given"
        );
    }
    let last = calls.last().expect("the newest call");
    assert_eq!(
        last.kept().back().map(|(_, line)| line.as_str()),
        Some("turn 500 line 1000"),
        "the newest line is the one it keeps last"
    );
}
