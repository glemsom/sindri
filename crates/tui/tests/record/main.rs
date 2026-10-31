//! The record in the frame: `/export` writes the current thread as markdown,
//! and the rail's session id can be copied out to the `sindri sessions show`
//! that already exists. Every fixture is a record under a temporary state
//! directory, written by the same `Recorder` a run writes with, so nothing here
//! reads or writes a real user's state.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken record reports itself"
)]

#[path = "../common/mod.rs"]
mod common;

pub use common::{notes, press, row_of, screen, type_text};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use sindri_core::agent::{Report, Stopped};
use sindri_core::paths::SessionDir;
use sindri_core::provider::Usage;
use sindri_core::record::{Cost, Recorder, Redaction, Start};
use sindri_core::sandbox::Enforcement;
use sindri_tui::copy::Clipboard;
use sindri_tui::{App, Launch};
use tempfile::TempDir;

fn launch(session: &str) -> Launch {
    Launch {
        session: session.to_string(),
        branch: Some("t33-the-record".to_string()),
        ..common::launch()
    }
}

/// One recording clipboard: what the frame copied, in the order it copied it.
#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<String>>>);

impl Clipboard for Recorded {
    fn copy(&self, text: &str) -> sindri_tui::copy::Copied {
        self.0.lock().expect("the clipboard").push(text.to_string());
        sindri_tui::copy::Copied::Tool("a fake")
    }
}

impl Recorded {
    fn taken(&self) -> Vec<String> {
        self.0.lock().expect("the clipboard").clone()
    }
}

fn report(stopped: Stopped) -> Report {
    Report {
        answer: String::new(),
        turns: 1,
        stopped,
        model: Some("claude-mock".to_string()),
        tool_calls: 0,
        usage: Usage::default(),
        duration: Duration::from_millis(10),
    }
}

/// One record, written by the recorder a run writes with. `stop` is `None` for
/// a run that never finished — a record still saying `running`.
fn record(state: &Path, model: &str, prompt: &str, answer: &str, stop: Option<Stopped>) -> String {
    let session = SessionDir::create_in(state).expect("a session directory");
    let recorder = Recorder::start(
        &session,
        Start::of(
            "anthropic".to_string(),
            model.to_string(),
            Enforcement::Bubblewrap,
        ),
        Redaction::new(Vec::<String>::new()),
    )
    .expect("the record starts");
    recorder.user(prompt);
    recorder.assistant(1, answer, None, Cost::default());
    if let Some(stopped) = stop {
        recorder.finish(&report(stopped)).expect("the record ends");
    }
    // The list is ordered by when a run started, and a millisecond is what
    // `session.json` records: two records started in the same one would sort
    // by id, and this test is about the order a reader sees.
    std::thread::sleep(Duration::from_millis(5));
    session.id().to_string()
}

struct Fixture {
    _state: TempDir,
    sessions: PathBuf,
    workspace: PathBuf,
    app: App,
    clipboard: Recorded,
}

impl Fixture {
    /// Three records, oldest first, plus a frame whose own session is the
    /// newest of them: `/export` writes the thread the frame is holding.
    fn three_records() -> Fixture {
        let state = TempDir::new().expect("a state directory");
        let workspace = state.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("a workspace");
        let oldest = record(
            state.path(),
            "claude-old",
            "how many files?",
            "Twelve files.",
            Some(Stopped::Final),
        );
        let middle = record(
            state.path(),
            "claude-middle",
            "rewrite the parser",
            "Halfway.",
            Some(Stopped::MaxTurns),
        );
        let newest = record(
            state.path(),
            "claude-mock",
            "why is the gate red?",
            "The record could not be read.",
            None,
        );
        let _ = (oldest, middle);
        let clipboard = Recorded::default();
        let app = App::with_clipboard(launch(&newest), None, Box::new(clipboard.clone()))
            .with_sessions(state.path().join("sessions"))
            .with_workspace(&workspace);
        let sessions = state.path().join("sessions");
        Fixture {
            _state: state,
            sessions,
            workspace,
            app,
            clipboard,
        }
    }
}

/// Sends a command the way a user sends one now: a bare name opens the offer
/// the catalogue makes, so the first `Enter` takes it and the second sends it.
/// A line with an argument opens no offer and costs one `Enter` — a second one
/// there would land on an empty draft, where `Enter` means something else.
fn command(app: &mut App, line: &str) {
    type_text(app, line);
    let offered = app.completion().is_some_and(|open| !open.items.is_empty());
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    if offered {
        press(app, KeyCode::Enter, KeyModifiers::NONE);
    }
}

fn click(app: &mut App, column: u16, row: u16) {
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

mod export;
