//! The hands, driven the way a user drives them: a drag that copies, a refusal
//! that says it refused, `@` completion, the `/` commands, `?`'s list of
//! bindings, and the spill file of an output too large to show (issue #17).
//!
//! Every copy goes to a clipboard the test owns: nothing here touches the
//! user's real clipboard (ADR-0013).

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sindri_core::agent::Event;
use sindri_tui::copy::{Clipboard, Copied};
use sindri_tui::driver::{CallOver, Finished, Spilled, Stop};
use sindri_tui::entry::Entry;
use sindri_tui::overlay::BINDINGS;
use sindri_tui::render;
use sindri_tui::{App, Launch};

/// A clipboard that records what it was handed, and answers as told.
#[derive(Clone)]
struct Fake {
    seen: Arc<Mutex<Vec<String>>>,
    answer: Copied,
}

impl Fake {
    fn taking() -> Fake {
        Fake {
            seen: Arc::new(Mutex::new(Vec::new())),
            answer: Copied::Sent,
        }
    }

    fn refusing() -> Fake {
        Fake {
            seen: Arc::new(Mutex::new(Vec::new())),
            answer: Copied::Refused("no tool on PATH took the text".to_string()),
        }
    }

    fn saw(&self) -> Vec<String> {
        self.seen
            .lock()
            .map(|seen| seen.clone())
            .unwrap_or_default()
    }
}

impl Clipboard for Fake {
    fn copy(&self, text: &str) -> Copied {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(text.to_string());
        }
        self.answer.clone()
    }
}

fn launch(session: &str) -> Launch {
    Launch {
        session: session.to_string(),
        temp: format!("/state/sindri/sessions/{session}/tmp"),
        branch: Some("main".to_string()),
        sandbox: "bwrap · 1 writable".to_string(),
        max_turns: 250,
        context_limit: Some(200_000),
    }
}

fn app_with(clipboard: Fake) -> App {
    App::with_clipboard(
        launch("4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77"),
        None,
        Box::new(clipboard),
    )
}

/// A frame with one question and one answer, drawn at 100×20.
fn answered(clipboard: Fake) -> App {
    let mut app = app_with(clipboard);
    app.note_info("a question");
    app.event(Event::TextDelta {
        turn: 1,
        text: "the answer, in full".to_string(),
    });
    app.finished(Finished {
        stopped: Stop::Final,
        turns: 1,
        ..Finished::default()
    });
    app
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

fn shown(rows: &[String]) -> String {
    rows.join("\n")
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.key(KeyEvent::new(code, modifiers));
}

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

/// The notes the frame has written, as one string.
fn notes(app: &App) -> String {
    app.entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Note(note) => Some(note.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_drag_copies_the_rendered_rows_and_nothing_else() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .position(|row| row.contains("the answer"))
        .expect("the answer is on screen");

    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        0,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        40,
        row as u16 + 1,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        40,
        row as u16 + 1,
    );

    let copied = clipboard.saw();
    assert_eq!(copied.len(), 1, "one drag is one copy: {copied:?}");
    assert_eq!(
        copied[0], "the answer, in full",
        "the copy is exactly the rows the drag covered"
    );
    assert!(
        !copied[0].contains('\u{1b}'),
        "and no escape sequence reaches it"
    );
}

#[test]
fn a_refused_copy_is_reported_as_a_refusal_and_never_as_success() {
    let mut app = answered(Fake::refusing());
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .position(|row| row.contains("the answer"))
        .expect("the answer is on screen");
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        0,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        30,
        row as u16,
    );
    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        30,
        row as u16,
    );

    let said = notes(&app);
    assert!(
        said.contains("copy refused"),
        "a refusal says it refused: {said:?}"
    );
    screen(&mut app, 100, 24);
    assert!(
        !shown(&screen(&mut app, 100, 24)).contains("copied"),
        "and nothing claims the copy worked"
    );
}

#[test]
fn ctrl_y_copies_the_last_answer_without_a_mouse() {
    let clipboard = Fake::taking();
    let mut app = answered(clipboard.clone());
    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(
        clipboard.saw(),
        vec!["the answer, in full".to_string()],
        "Ctrl-Y is the drag's keyboard twin"
    );
    assert!(
        notes(&app).contains("OSC 52"),
        "and it says how, without claiming a confirmation it cannot have: {:?}",
        notes(&app)
    );
}

#[test]
fn at_completes_workspace_paths_in_the_background() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "look at @src/ma");
    let (generation, typed) = app
        .take_walk()
        .expect("typing an @ path asks for a background walk");
    assert_eq!(typed, "src/ma", "the walk ranks against what was typed");

    // The walk answers on its own thread; the frame is told what it found.
    let found: Vec<String> = (0..20).map(|index| format!("src/main{index}.rs")).collect();
    app.paths(generation, found);
    assert_eq!(
        app.completion().map(|completion| completion.items.len()),
        Some(8),
        "about eight rows, never the tree"
    );
    let rows = shown(&screen(&mut app, 100, 24));
    assert!(
        rows.contains("src/main0.rs"),
        "the offers are drawn: {rows}"
    );

    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(
        app.composer().text(),
        "look at src/main0.rs",
        "Tab takes the offer and replaces what was typed after the @"
    );
    assert!(app.completion().is_none(), "and the offer closes");
    assert_eq!(app.queued(), 0, "accepting a path does not send the prompt");
}

#[test]
fn enter_takes_a_completion_and_esc_dismisses_one() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "@mai");
    let generation = app
        .take_walk()
        .map(|(generation, _)| generation)
        .unwrap_or(0);
    app.paths(
        generation,
        vec!["main.rs".to_string(), "src/main.rs".to_string()],
    );
    screen(&mut app, 100, 24);

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.completion().is_none(), "Esc dismisses the completion");
    assert_eq!(
        app.composer().text(),
        "@mai",
        "and changes nothing that was typed"
    );

    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    let job = app
        .start()
        .expect("Enter with no completion offered sends the prompt");
    assert_eq!(job.prompt, "@mai");
}

#[test]
fn a_dotfile_is_offered_only_when_a_dot_is_typed() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "@gi");
    let generation = app
        .take_walk()
        .map(|(generation, _)| generation)
        .unwrap_or(0);
    app.paths(
        generation,
        vec![".gitignore".to_string(), "gist.md".to_string()],
    );
    assert_eq!(
        app.completion().map(|completion| completion.items.clone()),
        Some(vec!["gist.md".to_string()])
    );

    let mut dotted = app_with(Fake::taking());
    type_text(&mut dotted, "@.gi");
    let generation = dotted
        .take_walk()
        .map(|(generation, _)| generation)
        .unwrap_or(0);
    dotted.paths(generation, vec![".gitignore".to_string()]);
    assert_eq!(
        dotted
            .completion()
            .map(|completion| completion.items.clone()),
        Some(vec![".gitignore".to_string()]),
        "a dot asks for dotfiles"
    );
}

#[test]
fn new_command_empties_the_transcript_and_asks_for_a_fresh_session() {
    let mut app = answered(Fake::taking());
    assert!(!app.entries().is_empty());
    let old = app.rail_guid();

    type_text(&mut app, "/new");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(app.entries().is_empty(), "the transcript is emptied");
    assert_eq!(app.composer().text(), "", "and the command is not a prompt");
    assert!(app.take_renew(), "the front end is asked for a new session");
    assert_eq!(
        app.rail_guid(),
        old,
        "the rail keeps the old GUID until the front end answers"
    );

    app.renewed(Some(launch("9a1e0c33-0000-4b93-9c0e-0d3f0e5c1a77")));
    assert_eq!(
        app.rail_guid(),
        "9a1e0c33-0000-4b93-9c0e-0d3f0e5c1a77",
        "a fresh session GUID is on the rail"
    );
    assert!(
        notes(&app).contains("new session 9a1e0c33"),
        "and said in the transcript: {:?}",
        notes(&app)
    );
}

#[test]
fn help_and_quit_are_commands_too() {
    let mut app = app_with(Fake::taking());
    type_text(&mut app, "/help");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.overlay().is_some(), "/help opens the list of bindings");
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    type_text(&mut app, "/quit");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.wants_quit(), "/quit leaves the frame");

    let mut unknown = app_with(Fake::taking());
    type_text(&mut unknown, "/settings");
    press(&mut unknown, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        notes(&unknown).contains("/settings: no such command"),
        "an unknown command is said in the transcript: {:?}",
        notes(&unknown)
    );
    assert!(!unknown.wants_quit(), "and leaves the frame alone");
}

#[test]
fn question_mark_lists_every_binding_and_esc_closes_it() {
    let mut app = app_with(Fake::taking());
    press(&mut app, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(app.overlay().is_some(), "? opens the help overlay");
    let rows = shown(&screen(&mut app, 100, 30));
    for binding in BINDINGS {
        assert!(
            rows.contains(binding.what),
            "the overlay lists {:?}: {rows}",
            binding.what
        );
    }
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay().is_none(), "Esc closes the overlay");
}

#[test]
fn an_output_too_large_to_show_offers_its_spill_file_and_opens_untruncated() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    let full: String = (0..500).map(|line| format!("full line {line}\n")).collect();
    std::fs::write(&path, full).expect("write the spill file");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("seq 500".to_string()),
    });
    app.over(
        CallOver::Exited(0),
        Spilled {
            truncated: true,
            files: vec![path.clone()],
        },
    );

    let rows = shown(&screen(&mut app, 100, 20));
    assert!(
        rows.contains("output too large"),
        "the call offers its spill file: {rows}"
    );
    assert!(
        rows.contains("Ctrl-O"),
        "and says how to open it from the keyboard: {rows}"
    );

    press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    let opened = shown(&screen(&mut app, 100, 30));
    assert!(
        opened.contains("full line 0"),
        "opening it shows the untruncated text: {opened}"
    );
    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);
    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.overlay().is_none(), "Esc closes the spill");
}

#[test]
fn a_click_on_the_spill_row_opens_it_like_ctrl_o() {
    let spills = tempfile::tempdir().expect("a temp directory");
    let path = spills.path().join("stdout");
    std::fs::write(&path, "one\ntwo\nthree\n").expect("write the spill file");

    let mut app = app_with(Fake::taking());
    app.event(Event::ToolCall {
        turn: 1,
        command: Some("seq 3".to_string()),
    });
    app.over(
        CallOver::Exited(0),
        Spilled {
            truncated: true,
            files: vec![path],
        },
    );
    let rows = screen(&mut app, 100, 20);
    let row = rows
        .iter()
        .position(|row| row.contains("output too large"))
        .expect("the affordance is on screen");
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        2,
        row as u16,
    );
    assert!(
        app.overlay().is_some(),
        "a click on the affordance opens what Ctrl-O opens"
    );
}
