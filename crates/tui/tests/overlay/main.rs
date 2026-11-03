//! The overlays own their space: the help list
//! wraps a description, sizes its box from the wrapped rows and scrolls
//! itself, the settings modal keeps its refusal and its keys on screen when the
//! list of rows does not fit, and a finished thought's whole text is a box the
//! frame's own page keys move. Every fixture points the frame at a temporary
//! config path, or at a thread it built itself.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

#[path = "../common/mod.rs"]
mod common;

pub use common::{any, launch, press, row_of, screen, shown, type_text};
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use sindri_core::agent::Event;
use sindri_core::config::{KEYS, Key};
use sindri_tui::overlay::{BINDINGS, Overlay, help_rows};
use sindri_tui::{App, Launch};
use tempfile::TempDir;

fn app() -> App {
    App::new(
        Launch {
            branch: Some("t31-the-overlays".to_string()),
            ..common::launch()
        },
        None,
    )
}

/// A frame over a temporary config file: the settings modal reads and writes
/// this path and nothing else.
struct Fixture {
    _dir: TempDir,
    path: PathBuf,
    app: App,
}

fn fixture(file: &str) -> Fixture {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("sindri/config.toml");
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the config dir");
    std::fs::write(&path, file).expect("the config file");
    let app = app().with_config(&path);
    Fixture {
        _dir: dir,
        path,
        app,
    }
}

/// A thread longer than any screen these tests draw: the transcript beneath an
/// overlay has somewhere to move to, so a page key that leaks out is visible.
fn thread(turns: usize) -> App {
    let mut app = app();
    for turn in 1..=turns {
        app.note_info(format!("question {turn:02}"));
    }
    app
}

/// `/settings`, typed and sent: the name opens the offer the catalogue makes,
/// the first `Enter` takes it and the second sends it.
fn open_settings(app: &mut App) {
    type_text(app, "/settings");
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

fn wheel(app: &mut App, kind: MouseEventKind) {
    app.mouse(MouseEvent {
        kind,
        column: 10,
        row: 10,
        modifiers: KeyModifiers::NONE,
    });
}

/// The box's own edge row, where a scrolling box states its position.
fn title(rows: &[String]) -> String {
    rows.iter()
        .find(|row| row.contains('╭'))
        .cloned()
        .expect("the overlay draws a box")
}

/// The help overlay, opened with the one key that opens it.
fn help(app: &mut App) {
    press(app, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(app.overlay().is_some(), "? opens the help overlay");
}

#[test]
fn every_binding_is_reachable_at_a_hundred_by_twenty_four_and_at_seventy_by_twenty() {
    for (width, height) in [(100u16, 24u16), (70, 20)] {
        let mut app = app();
        help(&mut app);
        let mut seen = String::new();
        for _ in 0..40 {
            let rows = screen(&mut app, width, height);
            seen.push_str(&shown(&rows));
            seen.push('\n');
            press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
            if screen(&mut app, width, height) == rows {
                break;
            }
        }
        assert_eq!(
            help_rows().len(),
            BINDINGS.len() + 1,
            "one row per binding, and one row for the commands"
        );
        for (keys, what) in help_rows() {
            assert!(
                seen.contains(keys.as_str()),
                "{width}×{height} can reach {keys:?} by scrolling: {seen}"
            );
            // And the whole of what it says, in order: a row cut at the border
            // is not a row that was reached, however visible its keys are. The
            // words are read in the order they were written rather than as one
            // line, because how many rows they wrap into is the box's business
            // and not the reader's.
            let mut at = 0;
            for word in what.split(' ') {
                match seen[at..].find(word) {
                    Some(found) => at += found + word.len(),
                    None => panic!("{width}×{height} cannot read {word:?} of {keys:?}: {seen}"),
                }
            }
        }
    }
}

#[test]
fn the_page_keys_and_the_wheel_scroll_the_help_and_not_the_frame_beneath_it() {
    let mut app = thread(40);
    help(&mut app);
    let top = screen(&mut app, 100, 20);
    assert!(any(&top, "send the prompt"), "the list starts at the top");

    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    wheel(&mut app, MouseEventKind::ScrollDown);
    let down = screen(&mut app, 100, 20);
    assert!(
        !any(&down, "send the prompt"),
        "PgDn and the wheel moved the list they were pressed in: {}",
        shown(&down)
    );
    assert!(
        any(&down, "↑ more"),
        "and the box says there is more above: {}",
        shown(&down)
    );

    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    wheel(&mut app, MouseEventKind::ScrollUp);
    let back = screen(&mut app, 100, 20);
    assert!(
        any(&back, "send the prompt"),
        "PgUp and the wheel came back to the top: {}",
        shown(&back)
    );

    // And the transcript under the box never moved: closing the help still
    // shows the newest thing the thread said, not a screen further up.
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    let frame = screen(&mut app, 100, 20);
    assert!(
        any(&frame, "question 40") && !any(&frame, "question 01"),
        "the frame beneath is where it was: {}",
        shown(&frame)
    );
}

#[test]
fn the_help_states_its_position_when_the_list_is_longer_than_the_box() {
    let mut app = app();
    help(&mut app);
    // Seventy columns wrap the list past the box, so the box says where the
    // reader is: the first row it shows, counted from one.
    let top = title(&screen(&mut app, 70, 20));
    assert!(
        top.contains("1 of "),
        "a scrolled list states its position: {top}"
    );
    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    let down = title(&screen(&mut app, 70, 20));
    assert!(
        down.contains(" of ") && !down.contains("· 1 of "),
        "a page moves the position it states: {down}"
    );
    assert!(
        any(&screen(&mut app, 70, 20), "↑ more"),
        "and the edge that has more says so"
    );

    // At a terminal the whole list fits in, there is no position to state.
    let mut roomy = self::app();
    help(&mut roomy);
    let all = title(&screen(&mut roomy, 100, 40));
    assert!(
        !all.contains(" of "),
        "a list that fits says nothing about where it is: {all}"
    );
}

#[test]
fn a_description_wraps_under_its_keys_instead_of_being_cut_at_the_edge() {
    let mut app = app();
    help(&mut app);
    // The longest descriptions do not fit beside their keys at seventy
    // columns: they wrap and are read, rather than losing their tail at the
    // border. The list is longer than the box, so both screens are looked at.
    let mut rows = screen(&mut app, 70, 20);
    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    rows.extend(screen(&mut app, 70, 20));
    let seen = shown(&rows);
    for wanted in [
        "copy the session id",
        "sessions show",
        "show` takes",
        "mark rendered rows",
        "a row at a time either way",
        "newest queued prompt taken back",
        "click the spill row",
        "whose own selection it is",
    ] {
        assert!(
            seen.contains(wanted),
            "a wrapped description keeps {wanted:?}: {seen}"
        );
    }
    // And the continuation starts under the description, not under the keys.
    let wrapped = rows
        .iter()
        .find(|row| row.contains("sessions show"))
        .expect("the wrapped row");
    let inside: String = wrapped
        .split('│')
        .nth(1)
        .unwrap_or_default()
        .chars()
        .take(10)
        .collect();
    assert!(
        inside.trim().is_empty(),
        "the continuation is indented under its column: {wrapped:?}"
    );
}

#[test]
fn a_failed_save_says_why_at_a_hundred_by_twelve() {
    let mut f = fixture("model = \"m\"\n");
    open_settings(&mut f.app);
    for _ in 0..5 {
        press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    }
    type_text(&mut f.app, "nope");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        f.app.overlay().is_some(),
        "a refused save keeps the modal open"
    );

    let rows = screen(&mut f.app, 100, 12);
    assert!(
        any(&rows, "not saved") && any(&rows, "max_turns"),
        "twelve rows still says why the save failed: {}",
        shown(&rows)
    );
    assert!(
        any(&rows, "Enter save"),
        "and the keys are pinned under it: {}",
        shown(&rows)
    );
    assert!(
        any(&rows, "↓ more"),
        "with the rows that did not fit marked: {}",
        shown(&rows)
    );
    assert_eq!(
        std::fs::read_to_string(&f.path).expect("the config file"),
        "model = \"m\"\n",
        "a refused save writes nothing"
    );
}

#[test]
fn the_modal_scrolls_to_a_chosen_row_past_the_fold_and_keeps_it_on_screen() {
    let mut f = fixture("model = \"m\"\n");
    open_settings(&mut f.app);

    // The schema's rows in a box of twelve: the last of them is past the fold.
    // Which row that is belongs to the schema, so the walk counts to the key.
    let last = KEYS
        .iter()
        .position(|key| *key == Key::ExtraHeaders)
        .expect("the schema has the key the modal ends on");
    for _ in 0..last {
        press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    }
    let rows = screen(&mut f.app, 100, 12);
    assert!(
        any(&rows, "extra_headers"),
        "the chosen row is on screen however long the list: {}",
        shown(&rows)
    );
    // What is pinned stays: the file it writes to, and the keys.
    assert!(
        any(&rows, "file "),
        "the file line is outside the scroll: {}",
        shown(&rows)
    );
    assert!(
        any(&rows, "Enter save"),
        "and the hint row is pinned too: {}",
        shown(&rows)
    );

    // Back up, and the first row is on screen again.
    for _ in 0..9 {
        press(&mut f.app, KeyCode::Up, KeyModifiers::NONE);
    }
    let up = screen(&mut f.app, 100, 12);
    assert!(
        any(&up, "provider"),
        "the list scrolls back with the cursor: {}",
        shown(&up)
    );
}

/// A finished thought of forty numbered lines: longer than any box these tests
/// draw, so a page key has somewhere to move to. The lines are numbered from
/// one with a leading zero, so a test that names one cannot match another.
fn a_long_thought() -> String {
    (1..=40).map(|n| format!("thought {n:02}\n")).collect()
}

#[test]
fn ctrl_r_opens_the_newest_thought_whole_and_esc_closes_it() {
    let mut app = app();
    app.event(Event::Reasoning {
        turn: 1,
        text: "an older thought\n".to_string(),
    });
    app.event(Event::Reasoning {
        turn: 2,
        text: a_long_thought(),
    });
    let thread = screen(&mut app, 100, 24);
    assert!(
        any(&thread, "thought 40"),
        "the newest thought is a row of the thread already: {thread:#?}"
    );

    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(
        app.overlay().is_some(),
        "Ctrl-R opens the whole of a finished thought over the frame"
    );
    let rows = screen(&mut app, 100, 24);
    assert!(
        any(&rows, "thought 01"),
        "on the newest thought's own text: {rows:#?}"
    );
    assert!(
        !any(&rows, "an older thought"),
        "and not the one before it: {rows:#?}"
    );

    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    let down = screen(&mut app, 100, 24);
    assert!(
        any(&down, "thought 40") && !any(&down, "thought 01"),
        "the page keys move the box by its screenful: {down:#?}"
    );
    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    assert!(
        any(&screen(&mut app, 100, 24), "thought 01"),
        "and back to the head of the thought: {:#?}",
        screen(&mut app, 100, 24)
    );
    wheel(&mut app, MouseEventKind::ScrollDown);
    assert!(
        !any(&screen(&mut app, 100, 24), "thought 01"),
        "the wheel moves the same box the keys do"
    );

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay().is_none(), "Esc closes it");
    let back = screen(&mut app, 100, 24);
    assert!(
        any(&back, "thought 40") && !any(&back, "thought 01"),
        "and the frame beneath is the thread it was, its thought still in it: {back:#?}"
    );

    // `Ctrl-R` is a second way to read text the frame already gives, and never
    // the only one: a thought is a row of the thread and has nothing to open,
    // so `Enter` neither folds it nor takes a row away.
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let kept = screen(&mut app, 100, 24);
    assert!(
        any(&kept, "thought 40"),
        "Enter opens nothing and takes nothing away: {kept:#?}"
    );
}

#[test]
fn a_click_on_a_thought_marks_it_and_ctrl_r_opens_the_newest() {
    let mut app = app();
    app.event(Event::Reasoning {
        turn: 1,
        text: "an older thought\n".to_string(),
    });
    app.event(Event::Reasoning {
        turn: 2,
        text: "the newer thought\n".to_string(),
    });
    // A thought is a row of the thread, so a click on it starts a mark as one on
    // an answer does: nothing folds, nothing is chosen, and `Ctrl-R` opens the
    // newest thought there is rather than one the reader touched.
    let rows = screen(&mut app, 100, 24);
    let older = row_of(&rows, "an older thought");
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 4,
        row: older as u16,
        modifiers: KeyModifiers::NONE,
    });
    app.mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 20,
        row: older as u16,
        modifiers: KeyModifiers::NONE,
    });
    assert!(
        !app.selection().is_empty(),
        "a drag over the thought's own row marks it, as it marks an answer's"
    );
    let marked = screen(&mut app, 100, 24);
    assert!(
        any(&marked, "an older thought") && any(&marked, "the newer thought"),
        "and both thoughts stay where they were: {marked:#?}"
    );

    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    let Some(Overlay::Reasoning(boxed)) = app.overlay() else {
        panic!("Ctrl-R opens a thought's box");
    };
    assert_eq!(
        boxed.lines,
        vec!["the newer thought".to_string()],
        "Ctrl-R opens the newest thought whole, and never the one marked"
    );
}
