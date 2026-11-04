//! The queue, made visible: a prompt typed during a run is drawn in a panel
//! above the composer, `↑` takes the newest one back and `Ctrl-X` drops it,
//! and `Ctrl-C` answers at once with `cancelling…` instead of waiting for the
//! run to land.

use super::*;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::buffer::Buffer;
use sindri_tui::theme;

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
fn up_at_the_top_of_a_scrolled_draft_walks_the_draft_and_leaves_the_queue() {
    let mut app = running();
    send(&mut app, "the older question");
    app.paste(&draft(20));
    // Five rows up puts the cursor on the top row the panel draws while the
    // draft still has rows above it: the pool `↑` draws from is the draft's,
    // not the panel's.
    for _ in 0..5 {
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Up, KeyModifiers::NONE);

    assert_eq!(
        app.queued(),
        1,
        "a cursor at the panel's top row still has the draft above it, so `↑` walks and takes nothing back"
    );
    assert_eq!(
        app.composer().text().lines().count(),
        20,
        "and the draft is the twenty rows it was"
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

    app.finished(ended(Stop::Cancelled, 1, Usage::default()));
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

/// The queued panel's own rectangle, found by its corner: a frame with a run in
/// flight and nothing open has no box above it but this one.
fn queue_box(buf: &Buffer) -> Rect {
    let corner = |row: u16| (0..buf.area.width).find(|column| buf[(*column, row)].symbol() == "╭");
    let top = (0..buf.area.height)
        .find(|row| corner(*row).is_some())
        .expect("the queue's corner is rounded");
    let left = corner(top).expect("the queue opens on its left");
    let right = (left..buf.area.width)
        .find(|column| buf[(*column, top)].symbol() == "╮")
        .expect("the queue closes on its right");
    let bottom = (top..buf.area.height)
        .find(|row| buf[(left, *row)].symbol() == "╰")
        .expect("the queue closes at the bottom");
    Rect {
        x: left,
        y: top,
        width: right - left + 1,
        height: bottom - top + 1,
    }
}

/// The words the panel holds, read off its own rows: the text inside its border,
/// joined the way a paragraph reads, with the `›` each row is marked by taken
/// away.
fn queued_words(buf: &Buffer, panel: Rect) -> Vec<String> {
    (panel.top() + 1..panel.bottom() - 1)
        .map(|y| {
            (panel.left() + 1..panel.right() - 1)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .filter(|word| *word != "›")
        .map(str::to_string)
        .collect()
}

#[test]
fn the_queue_ends_where_the_words_end() {
    // At a hundred columns the body splits into the words' 66, the seam's two
    // and the rail's 32; at a hundred and forty the words hold 106. The queue is
    // drawn in the first of them, as the composer is, so the two panels end on
    // one column and no cell of the rail's or the margin's is painted.
    let mut app = running();
    send(&mut app, "and then summarise them");

    for width in [100u16, 140] {
        let words = width - render::GAP_COLUMNS - RAIL as u16;
        let buf = buffer(&mut app, width, 24);
        let panel = queue_box(&buf);
        assert_eq!(
            panel.width, words,
            "the panel is as wide as the words at {width} columns: {panel:?}"
        );
        assert_eq!(
            panel.right(),
            words,
            "and its corner lands on the column the strip's light reaches"
        );
        for y in panel.top()..panel.bottom() {
            for x in panel.right()..width {
                assert_eq!(
                    buf[(x, y)].bg,
                    theme::CANVAS,
                    "({x}, {y}) is beside the queue, and the canvas the theme names is under it"
                );
                assert_eq!(
                    buf[(x, y)].symbol(),
                    " ",
                    "({x}, {y}) is canvas, as the rail's column below its last fact is"
                );
            }
        }
    }
}

#[test]
fn a_queued_prompt_longer_than_the_panel_wraps_inside_it() {
    let mut app = running();
    let prompt =
        "summarise every file the failing test touched and say which of them the fix rewrites";
    send(&mut app, prompt);
    let buf = buffer(&mut app, 100, 30);
    let panel = queue_box(&buf);

    assert_eq!(
        panel.width, MARGIN as u16,
        "a long prompt does not widen the panel: {panel:?}"
    );
    assert_eq!(
        panel.right(),
        MARGIN as u16,
        "and its corner lands on the column the strip's light reaches"
    );
    assert!(
        panel.height > 3,
        "it is wrapped over more than one row rather than cut: {panel:?}"
    );
    assert_eq!(
        queued_words(&buf, panel),
        prompt.split_whitespace().collect::<Vec<_>>(),
        "every word is inside the panel, at the width it was measured at"
    );
}

#[test]
fn below_the_rail_min_width_the_queue_keeps_the_whole_body() {
    let mut app = running();
    send(&mut app, "a queued sentence");
    let buf = buffer(&mut app, 90, 24);
    let panel = queue_box(&buf);

    assert_eq!(
        panel.width, 90,
        "the rail is a strip inside the body, so the words have all of it: {panel:?}"
    );
}
