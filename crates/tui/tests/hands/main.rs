//! The hands, driven the way a user drives them: a drag that copies, the mark
//! it leaves and what spends it, a refusal that says it refused, `@`
//! completion, the `/` offer, the `/` commands, `?`'s list of bindings, and the
//! spill file of an output too large to show. Every copy goes to a clipboard
//! the test owns: nothing here touches the user's real clipboard.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

#[path = "../common/mod.rs"]
mod common;

pub use common::{any, notes, press, screen, shown, type_text};
use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use sindri_core::agent::Event;
use sindri_tui::copy::{Clipboard, Copied};
use sindri_tui::driver::{CallOver, Finished, Spilled, Stop};
use sindri_tui::entry::Entry;
use sindri_tui::overlay::help_rows;
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
        ..common::launch()
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

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

mod commands;
mod completion;
mod copy;
mod marks;
mod prompts;
mod screen;
