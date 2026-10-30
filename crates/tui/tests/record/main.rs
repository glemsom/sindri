//! The record in the frame (issue #51): `/sessions` lists what every run left,
//! `Enter` reads one back without touching it, `/export` writes the current
//! thread as markdown, and the rail's session id can be copied out to the
//! `sindri sessions show` that already exists (ADR-0006, ADR-0011).
//!
//! Every fixture is a record under a temporary state directory, written by the
//! same `Recorder` a run writes with, so nothing here reads or writes a real
//! user's state.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken record reports itself"
)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sindri_core::agent::{Report, Stopped};
use sindri_core::paths::SessionDir;
use sindri_core::provider::Usage;
use sindri_core::record::{Recorder, Redaction, Start};
use sindri_core::sandbox::Enforcement;
use sindri_tui::copy::Clipboard;
use sindri_tui::entry::Entry;
use sindri_tui::overlay::Overlay;
use sindri_tui::render;
use sindri_tui::{App, Launch};
use tempfile::TempDir;

fn launch(session: &str) -> Launch {
    Launch {
        session: session.to_string(),
        temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
        branch: Some("t33-the-record".to_string()),
        sandbox: "bwrap · 1 writable".to_string(),
        workspace: "~/git/sindri".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        max_turns: 250,
        context_limit: Some(200_000),
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
        Start {
            provider: "anthropic".to_string(),
            model: model.to_string(),
            enforcement: Enforcement::Bubblewrap,
        },
        Redaction::new(Vec::<String>::new()),
    )
    .expect("the record starts");
    recorder.user(prompt);
    recorder.assistant(1, answer, None);
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

    /// Everything under the state directory, path and bytes: what a picker may
    /// read and may never change.
    fn state(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut all = Vec::new();
        let mut pending = vec![self._state.path().to_path_buf()];
        while let Some(next) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&next) else {
                continue;
            };
            for entry in entries {
                let path = entry.expect("an entry").path();
                if path.is_dir() {
                    pending.push(path.clone());
                    all.push((path, Vec::new()));
                } else {
                    all.push((path.clone(), std::fs::read(&path).unwrap_or_default()));
                }
            }
        }
        all.sort();
        all
    }
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.key(KeyEvent::new(code, modifiers));
}

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

fn command(app: &mut App, line: &str) {
    type_text(app, line);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

fn click(app: &mut App, column: u16, row: u16) {
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
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

fn row_of(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} is on screen: {}", shown(rows)))
}

mod export;
mod picker;
