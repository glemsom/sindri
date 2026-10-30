//! The queue, made visible: a prompt typed during a run is drawn in a panel
//! above the composer, `↑` takes the newest one back and `Ctrl-X` drops it,
//! and `Ctrl-C` answers at once with `cancelling…` instead of waiting for the
//! run to land.

use super::*;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.key(KeyEvent::new(code, modifiers));
}

fn send(app: &mut App, text: &str) {
    type_text(app, text);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

/// The row the composer's border opens on: everything above it is above the
/// composer, which is where the queue goes. It is found by its title, because
/// the queued panel has a rounded corner of its own above it.
fn composer(rows: &[String]) -> usize {
    rows.iter()
        .position(|row| row.contains("╭ prompt"))
        .expect("the composer draws")
}

/// Above the composer, and that is the only place the queued text may be.
fn above_composer(rows: &[String], needle: &str) -> bool {
    match rows.iter().position(|row| row.contains(needle)) {
        Some(at) => at < composer(rows),
        None => false,
    }
}

/// A frame with a run in flight: a prompt typed now queues.
fn running() -> App {
    let mut app = app();
    send(&mut app, "count the files");
    app.start().expect("the first run starts");
    app
}

#[test]
fn a_prompt_typed_mid_run_is_drawn_whole_above_the_composer() {
    let mut app = running();
    send(&mut app, "and then summarise them");
    let rows = screen(&mut app, 100, 24);

    assert!(
        above_composer(&rows, "and then summarise them"),
        "the whole queued prompt is on a panel above the composer: {rows:#?}"
    );
    assert!(
        rows.last().is_some_and(|row| !row.contains("queued")),
        "the footer no longer carries a count it cannot fit: {rows:#?}"
    );
}

#[test]
fn a_queued_prompt_that_spans_rows_is_drawn_whole() {
    let mut app = running();
    type_text(&mut app, "check the failing test");
    press(&mut app, KeyCode::Enter, KeyModifiers::SHIFT);
    type_text(&mut app, "and then fix it");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let rows = screen(&mut app, 100, 24);

    assert!(
        above_composer(&rows, "check the failing test"),
        "the first row of the prompt is queued: {rows:#?}"
    );
    assert!(
        above_composer(&rows, "and then fix it"),
        "and so is its second: a queued prompt is taken whole, never half: {rows:#?}"
    );
}

#[test]
fn a_queue_of_three_says_three_and_draws_each_of_them() {
    let mut app = running();
    for prompt in ["the first question", "the second question", "the third"] {
        send(&mut app, prompt);
    }
    let rows = screen(&mut app, 100, 28);

    for prompt in ["the first question", "the second question", "the third"] {
        assert!(
            above_composer(&rows, prompt),
            "every queued prompt is shown, whole: {prompt:?} in {rows:#?}"
        );
    }
    assert!(
        any(&rows, "3 queued"),
        "more than one is a count: {rows:#?}"
    );
}

#[test]
fn a_queue_past_the_rows_a_panel_can_hold_states_its_count() {
    let mut app = running();
    for prompt in [
        "one", "two", "three", "four", "five", "six", "seven", "eight",
    ] {
        send(&mut app, prompt);
    }
    let rows = screen(&mut app, 100, 24);

    assert_eq!(app.queued(), 8, "all eight wait their turn");
    assert!(
        any(&rows, "8 queued"),
        "the panel states how many it holds when it cannot hold them all: {rows:#?}"
    );
    assert!(
        above_composer(&rows, "one"),
        "the prompt that runs next is the one shown first: {rows:#?}"
    );
}

#[test]
fn up_returns_the_newest_queued_prompt_to_the_composer() {
    let mut app = running();
    send(&mut app, "the older question");
    send(&mut app, "the newer question");
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);

    assert_eq!(
        app.composer().text(),
        "the newer question",
        "↑ puts the newest queued prompt back where it was typed"
    );
    assert_eq!(app.queued(), 1, "and takes it out of the queue");
    let rows = screen(&mut app, 100, 24);
    assert!(
        rows[composer(&rows) + 1].contains("the newer question"),
        "the text is in the composer now, not in the panel: {rows:#?}"
    );
    assert!(
        above_composer(&rows, "the older question"),
        "the rest of the queue stays queued: {rows:#?}"
    );
}

#[test]
fn a_taken_back_prompt_lands_above_the_draft_rather_than_replacing_it() {
    let mut app = running();
    send(&mut app, "the queued question");
    type_text(&mut app, "a half-typed thought");
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);

    assert_eq!(
        app.composer().text(),
        "the queued question\na half-typed thought",
        "the queue is older than the draft, so it goes above it"
    );
}

#[test]
fn ctrl_x_drops_the_newest_queued_prompt_and_the_rest_stay() {
    let mut app = running();
    send(&mut app, "keep this one");
    send(&mut app, "drop this one");
    press(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);

    assert_eq!(app.queued(), 1, "Ctrl-X drops one, not the queue");
    let rows = screen(&mut app, 100, 24);
    assert!(
        !any(&rows, "drop this one"),
        "the dropped prompt is off the panel: {rows:#?}"
    );
    assert!(
        above_composer(&rows, "keep this one"),
        "the one that was not dropped is still queued: {rows:#?}"
    );
}

#[test]
fn ctrl_c_says_cancelling_at_once_and_clears_when_the_run_stops() {
    let mut app = running();
    type_text(&mut app, "a draft worth keeping");
    assert!(
        !any(&screen(&mut app, 100, 24), "cancelling…"),
        "a run that is not cancelling says nothing about it"
    );

    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    let rows = screen(&mut app, 100, 24);
    assert!(
        any(&rows, "cancelling…"),
        "the key is answered the moment it lands, not when the request does: {rows:#?}"
    );
    assert_eq!(
        app.composer().text(),
        "a draft worth keeping",
        "the draft stays in the composer"
    );

    app.finished(Finished {
        stopped: Stop::Cancelled,
        turns: 1,
        ..Finished::default()
    });
    let after = screen(&mut app, 100, 24);
    assert!(
        !any(&after, "cancelling…"),
        "the state clears when the run has actually stopped: {after:#?}"
    );
    assert!(
        any(&after, "a draft worth keeping"),
        "and the draft is still the user's: {after:#?}"
    );
}

#[test]
fn the_queued_text_survives_a_resize_a_redraw_and_a_second_ctrl_c() {
    let mut app = running();
    send(&mut app, "the queued sentence");
    assert!(any(&screen(&mut app, 100, 24), "the queued sentence"));

    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    for width in [90, 120] {
        let rows = screen(&mut app, width, 24);
        assert!(
            any(&rows, "the queued sentence"),
            "still queued and still drawn at {width} columns: {rows:#?}"
        );
    }
    assert_eq!(app.queued(), 1, "a redraw is not a drop");
}

#[test]
fn a_short_terminal_keeps_the_composer_and_the_count_rather_than_the_whole_queue() {
    let mut app = running();
    for prompt in ["one", "two", "three"] {
        send(&mut app, prompt);
    }
    let rows = screen(&mut app, 100, 10);

    assert!(
        rows.iter().any(|row| row.contains("╭ prompt")),
        "the composer is never the row the queue takes: {rows:#?}"
    );
    assert!(
        any(&rows, "3 queued"),
        "and what it cannot show, it counts: {rows:#?}"
    );
}
