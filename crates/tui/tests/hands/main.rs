//! The hands, driven the way a user drives them: a drag that copies, a
//! refusal that says it refused, `@` completion, the `/` commands, `?`'s list
//! of bindings, and the spill file of an output too large to show (issue
//! #17). Every copy goes to a clipboard the test owns: nothing here touches
//! the user's real clipboard (ADR-0013).

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
        workspace: "~/git/sindri".to_string(),
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

mod completion;
mod copy;
mod prompts;
mod screen;
