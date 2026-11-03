//! A thought is a row of the thread: its fragments land at the tail as they
//! arrive, its rows stay where they are when it ends, it is drawn in the
//! thought's own ink off the gutter an answer hangs off, and it is marked,
//! copied and scrolled back to by what does that to an answer. Nothing here
//! reads an internal: every assertion is about the rows a reader sees.

use super::*;

use std::time::Duration;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::buffer::Buffer;
use sindri_tui::entry::Entry;
use sindri_tui::theme;

/// The columns of the words at a terminal `width`: its own, less the rail and
/// the canvas between them, or the whole of it where the rail is a strip.
fn column(width: u16) -> u16 {
    if width >= render::RAIL_MIN_WIDTH {
        width - render::RAIL_COLUMNS - render::GAP_COLUMNS
    } else {
        width
    }
}

/// A settled turn behind the thought: a question, an answer, and the run that
/// stopped after it.
fn turn_behind(app: &mut App) {
    app.note_info("a question");
    app.event(Event::TextDelta {
        turn: 1,
        text: "the answer, in full".to_string(),
    });
    app.finished(ended(Stop::Final, 1, usage(0, 0, 0)));
}

/// `count` fragments of a thought, one a row, as the provider sends them.
fn thought(app: &mut App, turn: u32, count: usize) {
    for fragment in 1..=count {
        app.event(Event::ReasoningDelta {
            turn,
            text: format!("fragment {fragment}\n"),
        });
    }
}

/// The text `count` fragments leave the thought holding: one a line.
fn fragments(count: usize) -> String {
    (1..=count).map(|n| format!("fragment {n}\n")).collect()
}

/// The final reasoning event of a thought of `count` fragments, carrying the
/// whole of its text, as the provider sends it.
fn ended_thought(app: &mut App, turn: u32, count: usize) {
    app.event(Event::Reasoning {
        turn,
        text: fragments(count),
    });
}

/// The rows of a frame that carry a thought's words: the ones a reader is told
/// what the run is thinking on.
fn ink_rows(cells: &Buffer, width: u16, height: u16) -> Vec<u16> {
    (0..height)
        .filter(|row| {
            (0..width).any(|column| {
                let cell = &cells[(column, *row)];
                cell.symbol().chars().any(char::is_alphanumeric) && cell.fg == theme::THOUGHT
            })
        })
        .collect()
}

/// The column the first word of the row that says `said` is drawn in: the
/// gutter a turn's body hangs off, read off the frame rather than off the code.
fn first_word_column(cells: &Buffer, width: u16, height: u16, said: &str) -> u16 {
    let row = (0..height)
        .find(|row| {
            let line: String = (0..width).map(|x| cells[(x, *row)].symbol()).collect();
            line.contains(said)
        })
        .unwrap_or_else(|| panic!("no row of the frame says {said:?}"));
    (0..width)
        .find(|x| cells[(*x, row)].symbol() != " ")
        .unwrap_or_default()
}

#[test]
fn a_fragment_lands_at_the_tail_and_moves_no_row_above_it() {
    let mut app = app();
    turn_behind(&mut app);
    let settled = app.entries().len();
    let before: Vec<usize> = (0..settled)
        .map(|index| app.measure(index, column(100)))
        .collect();

    thought(&mut app, 2, 20);

    let after: Vec<usize> = (0..settled)
        .map(|index| app.measure(index, column(100)))
        .collect();
    assert_eq!(
        before, after,
        "no row above the thought is worth a row more or less than it was"
    );
    assert_eq!(
        app.entries().len(),
        settled + 1,
        "and the thought is one entry of the thread, not a surface beside it"
    );
    let rows = screen(&mut app, 100, 30);
    assert!(
        any(&rows, "fragment 20"),
        "the newest fragment is on the glass the moment it lands: {rows:#?}"
    );
    assert!(
        any(&rows, "⌁ thinking"),
        "under a marker row that says the run is thinking: {rows:#?}"
    );
}

#[test]
fn a_thought_that_has_ended_keeps_its_marker_and_its_text() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 3);
    let streaming = app.entries().len();
    ended_thought(&mut app, 2, 3);

    assert_eq!(
        app.entries().len(),
        streaming,
        "the thought is one entry before and after it ends: nothing arrives from \
         anywhere else"
    );
    let rows = screen(&mut app, 100, 30);
    assert_eq!(
        rows.iter().filter(|row| row.contains('⌁')).count(),
        1,
        "one marker row, and no second row saying what became of it: {rows:#?}"
    );
    assert!(
        any(&rows, "⌁ reasoned"),
        "the marker says the thought is over: {rows:#?}"
    );
    for fragment in ["fragment 1", "fragment 2", "fragment 3"] {
        assert!(
            any(&rows, fragment),
            "the text {fragment:?} stays in the transcript, where the reader read \
             it: {rows:#?}"
        );
    }
}

#[test]
fn a_thought_is_drawn_in_the_thoughts_ink_off_the_gutter_an_answer_hangs_off() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 3);

    let cells: Buffer = frame_cells(&mut app, 100, 30);
    let rows = ink_rows(&cells, 100, 30);
    assert!(
        rows.len() >= 4,
        "the marker and three fragments are drawn in the thought's ink: {rows:?}"
    );
    // The words start at the gutter an answer's do: the column an answer's
    // first word sits in, and never the terminal's first cell.
    let answer = first_word_column(&cells, 100, 30, "the answer, in full");
    let thought = first_word_column(&cells, 100, 30, "fragment 1");
    assert!(
        answer > 0 && answer == thought,
        "the thought hangs off the gutter an answer hangs off: {thought} against \
         {answer}"
    );
    assert_eq!(
        cells[(thought, rows[1])].fg,
        theme::THOUGHT,
        "and its first column is the thought's own ink: {:?}",
        cells[(thought, rows[1])]
    );
}

#[test]
fn the_marker_says_thinking_and_then_how_long_the_thought_took() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 3);
    app.pulse(Duration::from_millis(2400));

    let rows = screen(&mut app, 100, 30);
    let marker = row(&rows, "⌁");
    assert!(
        marker.contains("⌁ thinking") && marker.contains("2.4s"),
        "while the thought arrives its marker says what the run is doing and for \
         how long: {marker:?}"
    );

    app.pulse(Duration::from_millis(1800));
    ended_thought(&mut app, 2, 3);
    let rows = screen(&mut app, 100, 30);
    let marker = row(&rows, "⌁");
    assert!(
        marker.contains("⌁ reasoned") && marker.contains("4.2s"),
        "and once it is over, that it is over and how long it took: {marker:?}"
    );
    app.pulse(Duration::from_millis(900));
    assert_eq!(
        row(&screen(&mut app, 100, 30), "⌁"),
        marker,
        "which never moves again, however long the frame is on the clock after it"
    );
}

/// A run in flight with a thought in it, as the loop has it: the prompt is
/// sent and the provider has said `count` fragments of its thinking.
fn running(app: &mut App, count: usize) {
    type_text(app, "think about it");
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("a prompt starts a run");
    thought(app, 2, count);
    app.pulse(Duration::from_millis(1200));
}

#[test]
fn a_cancel_ends_the_thought_in_place_and_claims_only_the_time_it_spent() {
    let mut app = app();
    running(&mut app, 4);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

    let rows = screen(&mut app, 100, 30);
    assert!(
        any(&rows, "fragment 4"),
        "the text the reader was reading stays where it stood: {rows:#?}"
    );
    let marker = row(&rows, "⌁");
    assert!(
        marker.contains("⌁ reasoned") && marker.contains("1.2s"),
        "and the marker claims the clock the run spent on it and no more: {marker:?}"
    );
    assert!(
        !marker.contains("thinking"),
        "no row is left saying the run is still thinking: {marker:?}"
    );
}

#[test]
fn a_new_session_takes_the_thought_with_the_thread_it_belonged_to() {
    let mut app = app();
    running(&mut app, 4);
    app.renewed(Ok(launch()));

    let rows = screen(&mut app, 100, 30);
    assert!(
        !any(&rows, "fragment 4") && !any(&rows, "⌁"),
        "a new session's thread holds nothing of the thought it superseded, and \
         no marker row claiming a duration: {rows:#?}"
    );
}

#[test]
fn a_following_answer_ends_the_thought_where_it_stands() {
    let mut app = app();
    running(&mut app, 4);
    app.event(Event::TextDelta {
        turn: 2,
        text: "the answer starts".to_string(),
    });

    let rows = screen(&mut app, 100, 30);
    let marker = row(&rows, "⌁");
    assert!(
        marker.contains("⌁ reasoned 1.2s"),
        "the thought ends on the first answer fragment, with the clock it spent: \
         {marker:?}"
    );
    assert!(
        any(&rows, "fragment 4"),
        "and its rows stay where they were: {rows:#?}"
    );
    assert!(
        any(&rows, "the answer starts"),
        "so the answer is drawn under them, at the tail: {rows:#?}"
    );
    assert!(
        row_of(&rows, "⌁ reasoned") < row_of(&rows, "the answer starts"),
        "in that order: {rows:#?}"
    );
}

#[test]
fn a_tool_call_ends_the_thought_where_it_stands() {
    let mut app = app();
    running(&mut app, 4);
    app.event(Event::ToolCall {
        turn: 2,
        asked: Asked::Bash {
            command: "cargo test".to_string(),
        },
    });

    let rows = screen(&mut app, 100, 30);
    assert!(
        row(&rows, "⌁").contains("⌁ reasoned 1.2s"),
        "a call announced under a thought ends it, with the clock it spent: {rows:#?}"
    );
    assert!(
        any(&rows, "fragment 4"),
        "and leaves its rows where they stood: {rows:#?}"
    );
}

#[test]
fn a_run_that_ends_mid_thought_ends_it_in_place() {
    let mut app = app();
    running(&mut app, 4);
    app.finished(ended(Stop::Cancelled, 2, usage(0, 0, 0)));

    let rows = screen(&mut app, 100, 30);
    let marker = row(&rows, "⌁");
    assert!(
        marker.contains("⌁ reasoned") && marker.contains("1.2s"),
        "a run that ends inside a thought leaves a marker that says it is over \
         and the time it spent, and never that it is still thinking: {marker:?}"
    );
    assert!(
        any(&rows, "fragment 4"),
        "with the thought's rows still in the transcript: {rows:#?}"
    );
}

#[test]
fn the_frame_reserves_no_region_for_a_thought() {
    let mut app = app();
    turn_behind(&mut app);
    let quiet = screen(&mut app, 100, 30);
    let before = app.viewport().area;

    thought(&mut app, 2, 3);
    let rows = screen(&mut app, 100, 30);
    let after = app.viewport().area;

    assert_eq!(
        (before.x, before.width),
        (after.x, after.width),
        "the transcript keeps its columns and the rail its own: {rows:#?}"
    );
    assert_eq!(
        before.y + before.height,
        after.y + after.height,
        "and its floor is the same row with a thought in the thread as without \
         one: {rows:#?}"
    );
    assert_eq!(
        row_of(&rows, "╭ prompt"),
        (after.y + after.height) as usize,
        "the floor is the composer's own row, which no thought takes a row of: \
         {rows:#?}"
    );
    assert_eq!(
        row_of(&quiet, "╭ prompt"),
        row_of(&rows, "╭ prompt"),
        "so the composer has not moved for it either"
    );
}

#[test]
fn a_frame_asks_for_a_tick_only_while_a_run_is_in_flight() {
    let mut app = app();
    assert!(!app.ticks(), "a still frame asks for no tick");
    thought(&mut app, 2, 4);
    assert!(
        !app.ticks(),
        "a thought that is a row of the thread asks for no tick of its own"
    );
    ended_thought(&mut app, 2, 4);
    assert!(!app.ticks(), "and an ended one asks for none either");

    running(&mut app, 2);
    assert!(app.ticks(), "a run in flight is what the clock is for");
    app.finished(ended(Stop::Final, 2, usage(0, 0, 0)));
    assert!(!app.ticks(), "and its end takes the clock with it");
}

#[test]
fn a_thought_is_reached_by_scrolling_back_to_it() {
    let mut app = app();
    // A thought with ten turns said after it: far enough above the tail that no
    // frame holds it and the reader has to go back to it.
    thought(&mut app, 1, 6);
    ended_thought(&mut app, 1, 6);
    for turn in 2..=12 {
        app.note_info(format!("question {turn}"));
        app.event(Event::TextDelta {
            turn,
            text: format!("answer {turn}"),
        });
    }
    let tail = screen(&mut app, 100, 20);
    assert!(
        !any(&tail, "fragment 6"),
        "the thought is above the window the tail shows: {tail:#?}"
    );

    for _ in 0..8 {
        if any(&screen(&mut app, 100, 20), "fragment 6") {
            break;
        }
        press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    }
    let back = screen(&mut app, 100, 20);
    assert!(
        any(&back, "fragment 6") && any(&back, "⌁ reasoned"),
        "the thought is a row of the document, so scrolling back reaches it: {back:#?}"
    );
}

#[test]
fn a_thought_of_any_length_is_a_row_of_the_thread_whole() {
    // One fragment of six hundred characters, so ten rows of the transcript's
    // columns: nothing of it is cut, because a thought in the thread is the
    // document and not a pane standing over it.
    let whole: String = (1..=120).map(|n| format!("w{n:03} ")).collect();
    let mut app = app();
    app.event(Event::ReasoningDelta {
        turn: 1,
        text: whole.clone(),
    });
    let entry = app.entries().len() - 1;
    let rows = app.measure(entry, column(100));
    assert!(
        rows > 5,
        "a thought of ten rows is worth ten rows of the thread: {rows}"
    );
    let rows = shown(&screen(&mut app, 100, 30));
    assert!(
        rows.contains("w120"),
        "and its newest words are the ones at the tail: {rows:#?}"
    );
}

#[test]
fn a_thought_is_one_entry_of_the_document_and_not_a_state_beside_it() {
    let mut app = app();
    for turn in 1..=3 {
        app.event(Event::ReasoningDelta {
            turn,
            text: format!("thought {turn}\n"),
        });
        app.event(Event::Reasoning {
            turn,
            text: format!("thought {turn}\n"),
        });
    }
    let thoughts = app
        .entries()
        .iter()
        .filter(|entry| matches!(entry, Entry::Reasoning { .. }))
        .count();
    assert_eq!(
        thoughts, 3,
        "every thought the run had is an entry of the thread, in the order it had \
         them"
    );
}
