//! The pane: the run's surface for a thought, at the tail of the transcript and
//! hanging off the same two-column gutter a turn's answer and its calls hang
//! off. A thought is a state of the run and never an entry of the thread, so no
//! row of the thread reflows when one begins or ends, and the pane takes no
//! key: the reader's keys keep going to the composer.

use super::*;

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::buffer::Buffer;
use sindri_tui::pulse;
use sindri_tui::render::pane;
use sindri_tui::render::transcript;
use sindri_tui::theme;

/// A settled turn: a question, an answer, and the run that stopped after it.
fn turn_behind(app: &mut App) {
    app.note_info("a question");
    app.event(Event::TextDelta {
        turn: 1,
        text: "the answer, in full".to_string(),
    });
    app.finished(ended(Stop::Final, 1, usage(0, 0, 0)));
}

/// The columns of the words at a terminal `width`: its own, less the rail and
/// the canvas between them, or the whole of it where the rail is a strip.
fn column(width: u16) -> u16 {
    if width >= render::RAIL_MIN_WIDTH {
        width - render::RAIL_COLUMNS - render::GAP_COLUMNS
    } else {
        width
    }
}

/// The rows the pane stands on: the raised surface of the transcript's column.
/// The rail's chip is raised too, and it is the rail's column, so reading the
/// frame's first column is reading the pane and nothing else.
fn pane_rows(app: &mut App, width: u16, height: u16) -> Vec<u16> {
    let cells: Buffer = frame_cells(app, width, height);
    (0..height)
        .filter(|row| cells[(0, *row)].bg == theme::RAISED)
        .collect()
}

/// What the thought says, one fragment a row, `count` of them.
fn thought(app: &mut App, turn: u32, count: usize) {
    for fragment in 1..=count {
        app.event(Event::ReasoningDelta {
            turn,
            text: format!("fragment {fragment}\n"),
        });
    }
}

/// Lets the frame's clock on by `ticks` of the pulse, as the loop does while a
/// run is live: the pane's height is a reading of the clock and of nothing
/// else, so this is the whole of what drives it.
fn tick(app: &mut App, ticks: usize) {
    for _ in 0..ticks {
        app.pulse(pulse::TICK);
    }
}

/// The pane's height, read off the frame the clock has just drawn: one number a
/// tick, which is what a film is made of.
fn standing(app: &mut App) -> usize {
    pane_rows(app, 100, 30).len()
}

/// The final reasoning event of a thought of `count` fragments, carrying the
/// text the pane was given: the whole of it, as the provider sends it.
fn done(app: &mut App, turn: u32, count: usize) {
    let text: String = (1..=count).map(|n| format!("fragment {n}\n")).collect();
    app.event(Event::Reasoning { turn, text });
}

#[test]
fn twenty_fragments_of_a_thought_leave_the_threads_measured_rows_alone() {
    let mut app = app();
    turn_behind(&mut app);
    let settled = app.entries().len();
    let before: Vec<usize> = (0..settled)
        .map(|index| app.measure(index, column(100)))
        .collect();
    let quiet = screen(&mut app, 100, 30);

    thought(&mut app, 2, 20);

    let after: Vec<usize> = (0..app.entries().len())
        .map(|index| app.measure(index, column(100)))
        .collect();
    assert_eq!(
        app.entries().len(),
        settled,
        "a thought is not an entry: the thread holds what it held"
    );
    assert_eq!(
        before, after,
        "and every row of it is worth what it was worth before the thought"
    );

    let rows = screen(&mut app, 100, 30);
    let pane = pane_rows(&mut app, 100, 30);
    assert_eq!(
        pane.len() as u16,
        1 + pane.last().copied().unwrap_or(0) - pane.first().copied().unwrap_or(0),
        "the pane is one block of rows and not a scattering: {pane:?}"
    );
    assert!(
        any(&rows, "the answer, in full"),
        "the thread is still on screen above it: {rows:#?}"
    );
    assert_eq!(
        quiet
            .iter()
            .filter(|row| row.contains("a question") || row.contains("the answer"))
            .count(),
        rows.iter()
            .filter(|row| row.contains("a question") || row.contains("the answer"))
            .count(),
        "no row of the thread is lost to the pane"
    );
}

#[test]
fn a_reader_scrolled_back_keeps_every_row_they_were_reading() {
    let mut app = app();
    for turn in 1..=20 {
        app.note_info(format!("question {turn}"));
        app.event(Event::TextDelta {
            turn,
            text: format!("answer {turn}"),
        });
    }
    screen(&mut app, 100, 30);
    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    let held = screen(&mut app, 100, 30);

    thought(&mut app, 21, 20);

    let thinking = screen(&mut app, 100, 30);
    let pane = pane_rows(&mut app, 100, 30);
    let above = pane.first().copied().unwrap_or_default() as usize;
    assert!(
        above > 0,
        "the thread still has rows on the glass: {thinking:#?}"
    );
    // The words' own columns: the rail counts the tokens the thought has spent,
    // which is a figure and not a row of the thread.
    let words = |rows: &[String]| -> Vec<String> {
        rows.iter()
            .take(above)
            .map(|row| row.chars().take(column(100) as usize).collect())
            .collect()
    };
    assert_eq!(
        words(&thinking),
        words(&held),
        "the rows the reader was reading are the rows on the glass: the pane takes \
         its rows from the floor of the column and moves none of them"
    );
}

#[test]
fn a_thought_is_one_pane_at_the_tail_between_five_rows_and_eight() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 1);
    tick(&mut app, 4);
    let short = pane_rows(&mut app, 100, 30);
    assert_eq!(
        short.len(),
        5,
        "a thought with a line of text settles at the floor of its height: {short:?}"
    );

    thought(&mut app, 2, 40);
    tick(&mut app, 4);
    let tall = pane_rows(&mut app, 100, 30);
    assert_eq!(
        tall.len(),
        8,
        "and a thought of forty fragments is the most a thought may cost: {tall:?}"
    );

    // The floor of the tail: the last row of the pane is the last row of the
    // transcript's column, and the composer's panel opens under it.
    let rows = screen(&mut app, 100, 30);
    let last = tall.last().copied().unwrap_or_default();
    assert!(
        rows[last as usize + 1].contains('╭'),
        "the composer's panel opens on the row under the pane: {rows:#?}"
    );
    assert!(
        tall.first().copied().unwrap_or_default() > 0,
        "and the thread has rows above it: {tall:?}"
    );
}

#[test]
fn the_panes_head_names_the_thought_and_the_newest_fragment_is_on_its_last_row() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 20);
    tick(&mut app, 8);

    let rows = screen(&mut app, 100, 30);
    let pane = pane_rows(&mut app, 100, 30);
    let head = pane.first().copied().unwrap_or_default() as usize;
    assert!(
        rows[head].contains("⌁ thinking"),
        "the head row says what the run is doing: {:?}",
        rows[head]
    );
    assert!(
        rows[head].contains("0.9s"),
        "with the time the thought has taken: {:?}",
        rows[head]
    );
    let last = pane.last().copied().unwrap_or_default() as usize;
    assert!(
        rows[last].contains("fragment 20"),
        "the newest fragment is on the pane's last row, where a fragment never moves it: {:?}",
        rows[last]
    );
    assert!(
        !any(&rows, "fragment 1\n"),
        "and the rows that left the top are cut and not kept: {rows:#?}"
    );
}

#[test]
fn a_folded_thought_leaves_one_scar_and_no_fragment_in_the_thread() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 20);
    tick(&mut app, 8);
    app.event(Event::Reasoning {
        turn: 2,
        text: "fragment 1\nfragment 20\n".to_string(),
    });
    // One tick of hold and four of the fold: the pane is gone on the fifth.
    tick(&mut app, 5);

    assert!(app.thought().is_none(), "the thought is over");
    let rows = screen(&mut app, 100, 30);
    assert!(
        pane_rows(&mut app, 100, 30).is_empty(),
        "the pane has folded away: {rows:#?}"
    );
    let scars: Vec<&String> = rows.iter().filter(|row| row.contains('⌁')).collect();
    assert_eq!(
        scars.len(),
        1,
        "exactly one scar stands where the pane was: {scars:#?}"
    );
    assert!(
        scars[0].contains("⌁ reasoned"),
        "and it is the one row a finished thought leaves: {:?}",
        scars[0]
    );
    assert!(
        !any(&rows, "fragment 20"),
        "with nothing of the thought left in the thread: {rows:#?}"
    );
}

#[test]
fn a_thought_costs_the_pane_and_never_a_remeasure_of_the_thread() {
    // A thread short enough for the column to hold it with a pane over it, so
    // both frames lay out the same entries and differ only in the rows below
    // them: what the two views report is the thread's work and not the pane's.
    let mut quiet = app();
    turn_behind(&mut quiet);
    let mut thinking = app();
    turn_behind(&mut thinking);
    thought(&mut thinking, 2, 20);
    let rows = pane::height(
        thinking.thought().expect("the run is thinking"),
        column(100),
        thinking.now(),
    );

    let height = 27;
    transcript::window(&mut quiet, column(100), height);
    transcript::window(&mut thinking, column(100), height - rows as usize);
    let quiet = transcript::window(&mut quiet, column(100), height);
    let thinking = transcript::window(&mut thinking, column(100), height - rows as usize);

    assert_eq!(
        thinking.measured, quiet.measured,
        "the pane changes the column's height and not the entries it holds"
    );
    assert_eq!(
        thinking.remeasured, quiet.remeasured,
        "so nothing of the thread is measured again for it"
    );
}

#[test]
fn the_pane_draws_at_a_hundred_columns_and_where_the_rail_is_a_strip() {
    for width in [100, 90] {
        let mut app = app();
        turn_behind(&mut app);
        thought(&mut app, 2, 20);
        tick(&mut app, 8);
        let rows = screen(&mut app, width, 30);
        let pane = pane_rows(&mut app, width, 30);
        assert_eq!(
            pane.len(),
            8,
            "the pane is drawn at {width} columns: {rows:#?}"
        );
        assert!(
            any(&rows, "⌁ thinking"),
            "the head row is drawn at {width} columns: {rows:#?}"
        );
    }
}

#[test]
fn a_call_an_answer_a_cancel_and_a_new_session_all_end_the_thought() {
    // The pane's life is the run's, and one door closes it: each of these is a
    // way a thought stops being the thing happening, and each leaves no pane.
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 20);
    app.event(Event::ToolCall {
        turn: 2,
        asked: Asked::Bash {
            command: "cargo test".to_string(),
        },
    });
    assert!(
        app.thought().is_none(),
        "a call announced under the thought ends it"
    );

    thought(&mut app, 3, 20);
    app.event(Event::TextDelta {
        turn: 3,
        text: "an answer begins".to_string(),
    });
    assert!(
        app.thought().is_none(),
        "the answer beginning ends it: what is said is the thread's"
    );

    // A cancel exists while a run does: the prompt starts one, and the key is
    // answered the moment it lands rather than when the loop stops.
    type_text(&mut app, "think about it");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("a prompt starts a run");
    thought(&mut app, 4, 20);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(
        app.thought().is_none(),
        "a cancel is answered when the key lands, thought and all"
    );

    thought(&mut app, 5, 20);
    app.renewed(Ok(launch()));
    assert!(
        app.thought().is_none(),
        "and `/new` starts over with no thought of the last session"
    );
}

#[test]
fn the_pane_takes_no_key_and_the_composer_keeps_them() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 20);

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(
        app.thought().is_some(),
        "Esc is not the pane's: the run is not finished with the thought"
    );
    assert!(
        !pane_rows(&mut app, 100, 30).is_empty(),
        "so the pane is still drawn"
    );

    type_text(&mut app, "hi");
    assert_eq!(
        app.composer().text(),
        "hi",
        "and the keys go to the composer, as they always did"
    );
}

#[test]
fn a_pane_unfurls_one_row_a_tick_and_then_follows_its_text() {
    let mut app = app();
    turn_behind(&mut app);
    // Four fragments are the five rows a pane settles at — its head and four of
    // text — so for the first ticks the clock is the only thing that moves.
    thought(&mut app, 2, 4);

    let mut heights = vec![standing(&mut app)];
    for nth in 1..8 {
        tick(&mut app, 1);
        if nth == 7 {
            // A fifth fragment: the pane is past its floor and follows its text.
            app.event(Event::ReasoningDelta {
                turn: 2,
                text: "fragment 5\n".to_string(),
            });
        }
        heights.push(standing(&mut app));
    }

    assert_eq!(
        heights,
        vec![1, 2, 3, 4, 5, 5, 5, 6],
        "the pane unfurls one row a tick to its floor and then follows its text"
    );
}

#[test]
fn the_pane_folds_down_to_one_row_and_then_is_gone() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 4);
    tick(&mut app, 4);
    assert_eq!(standing(&mut app), 5, "the pane has unfurled to its floor");

    done(&mut app, 2, 4);

    let mut heights = vec![standing(&mut app)];
    for _ in 0..4 {
        tick(&mut app, 1);
        heights.push(standing(&mut app));
    }
    assert_eq!(
        heights,
        vec![5, 4, 3, 2, 1],
        "the fold holds for one tick and then gives up a row a tick"
    );

    tick(&mut app, 1);
    assert!(app.thought().is_none(), "and the thought is over with it");
    assert!(
        pane_rows(&mut app, 100, 30).is_empty(),
        "so the raised rows at the pane's floor are the thread's again"
    );
}

#[test]
fn the_scar_is_drawn_on_the_tick_the_pane_is_one_row() {
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 4);
    tick(&mut app, 4);
    done(&mut app, 2, 4);
    // The hold and three rows of the fold: the pane is one row on the fourth.
    tick(&mut app, 4);

    let pane = pane_rows(&mut app, 100, 30);
    assert_eq!(pane.len(), 1, "the pane has folded down to its last row");
    let rows = screen(&mut app, 100, 30);
    let scar = row_of(&rows, "⌁ reasoned");
    assert_eq!(
        scar as u16 + 1,
        pane[0],
        "the scar stands on the row the pane's own descent ends at — directly \
         above its last one — so the eye reads one movement and not a \
         disappearance and a separate arrival: {rows:#?}"
    );
    assert!(
        rows[pane[0] as usize].contains("⌁ thinking"),
        "with the pane's head still on its last row: {:?}",
        rows[pane[0] as usize]
    );
}

#[test]
fn a_frame_asks_for_no_tick_once_the_thought_has_folded() {
    let mut app = app();
    assert!(!app.ticks(), "a still frame is not on the clock");
    thought(&mut app, 2, 4);
    assert!(app.ticks(), "a thought is something for the clock to say");

    done(&mut app, 2, 4);
    tick(&mut app, 5);

    assert!(app.thought().is_none(), "the pane has folded away");
    assert!(
        !app.ticks(),
        "and a frame with no thought on it asks for no tick at all"
    );
}

#[test]
fn a_run_that_ends_inside_the_fold_still_leaves_one_scar() {
    // The fold is watched only while the frame has the ticks to watch it, and a
    // run can end inside it: the thought finished, so its scar is owed.
    let mut app = app();
    turn_behind(&mut app);
    thought(&mut app, 2, 4);
    tick(&mut app, 4);
    done(&mut app, 2, 4);
    app.finished(ended(Stop::Final, 2, usage(0, 0, 0)));

    assert!(app.thought().is_none(), "the pane goes with the run");
    let rows = screen(&mut app, 100, 30);
    let scars: Vec<&String> = rows.iter().filter(|row| row.contains('⌁')).collect();
    assert_eq!(
        scars.len(),
        1,
        "and the one row the thought is worth is in the thread: {scars:#?}"
    );
    assert!(
        scars[0].contains("⌁ reasoned"),
        "the scar a finished thought leaves: {:?}",
        scars[0]
    );
}

#[test]
fn a_thought_that_never_finished_leaves_no_scar() {
    // A cancel is answered the moment the key lands: nothing said the thought
    // was done, so the thread holds no word of it and a caught fold is not
    // invented for it.
    let mut app = app();
    turn_behind(&mut app);
    type_text(&mut app, "think about it");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("a prompt starts a run");
    thought(&mut app, 2, 4);
    tick(&mut app, 4);
    press(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);

    assert!(app.thought().is_none(), "the pane goes with the cancel");
    assert!(
        !any(&screen(&mut app, 100, 30), "⌁ reasoned"),
        "and no scar is written for a thought that never finished"
    );
}
