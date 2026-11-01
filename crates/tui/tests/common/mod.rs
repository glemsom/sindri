//! The harness the terminal UI's tests share: one frame drawn into a test
//! backend and read back as rows, the keys and the typing that drive it, the
//! readings a test takes of what a reader sees, and the fixtures a frame is
//! built from. There is one definition of each so that a change to how a frame
//! is captured is one edit, and the copies cannot drift apart — a test binary
//! takes the slice it uses.

#![allow(
    dead_code,
    reason = "each test binary uses a different slice of the harness"
)]
#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a broken fixture reports itself"
)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use sindri_core::agent::Event;
use sindri_tui::driver::{CallOver, Finished, Side, Spilled, Stop};
use sindri_tui::entry::Entry;
use sindri_tui::render;
use sindri_tui::{App, Launch, theme};

/// The session the shared fixture names; a test that needs another says so.
const SESSION: &str = "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77";

/// A launch the frame can draw a first frame from: the facts it names are the
/// ones a test asserts on, so they are written once. A test that needs a
/// different branch or session says so and takes the rest from here.
pub fn launch() -> Launch {
    Launch {
        session: SESSION.to_string(),
        temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
        branch: Some("main".to_string()),
        sandbox: "bwrap · 1 writable".to_string(),
        workspace: "~/git/sindri".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-6".to_string(),
        max_turns: 250,
        context_limit: Some(200_000),
    }
}

/// A launch whose run left the sandbox — `--yolo-unsafe` — so a test can see
/// what the frame says about a run that gave the boundary up. It is named here
/// because the shared literal is read whole, and a test may rename only the
/// facts it asserts on.
pub fn unsafe_launch() -> Launch {
    Launch {
        sandbox: "unsafe · your own privileges".to_string(),
        ..launch()
    }
}

pub fn app() -> App {
    App::new(launch(), None)
}

/// A frame drawn at `width` by `height`, read back as its cells: the escape a
/// picture leaves in a cell is kept, because a style or a mark is asserted on
/// where a row alone cannot say it.
pub fn buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

/// A frame drawn at `width` by `height`, read back as the frame's own cells
/// rather than as a backend's diff of them: a wide glyph's second cell is
/// skipped by a diff, so a style laid on it — a wash, a mark — can only be read
/// here.
pub fn frame_cells(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    let mut cells = Buffer::empty(Rect::new(0, 0, width, height));
    terminal
        .draw(|frame| {
            render::draw(app, frame);
            cells = frame.buffer_mut().clone();
        })
        .expect("the frame draws");
    cells
}

/// A frame drawn at `width` by `height`, read back as its rows: what the user
/// sees, one string per row.
pub fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let cells = buffer(app, width, height);
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| glyph(&cells[(x, y)]))
                .collect::<String>()
        })
        .collect()
}

/// A cell as a row reads. A picture is drawn as an escape sequence in the first
/// cell of each of its rows, which a test backend cannot paint, so it reads as
/// the one mark that says a graphic is there.
fn glyph(cell: &ratatui::buffer::Cell) -> &str {
    if cell.symbol().contains('\u{1b}') {
        "\u{25A9}"
    } else {
        cell.symbol()
    }
}

/// The ink the posture chip is drawn in, wherever the frame wears it — the
/// rail's posture row or the first frame's head: the chip is one raised run
/// from its `⬢` to its last cell, and that run's colour is the frame's only
/// word on whether the run is still inside its boundary.
pub fn posture_ink(app: &mut App, width: u16, height: u16) -> Color {
    let cells = buffer(app, width, height);
    let glyph = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .find(|(x, y)| cells[(*x, *y)].symbol() == "⬢")
        .expect("the frame wears the posture's glyph");
    let (x, y) = glyph;
    let ink: Vec<Color> = (x..width)
        .take_while(|x| cells[(*x, y)].bg == theme::RAISED)
        .map(|x| cells[(x, y)].fg)
        .collect();
    assert!(
        ink.len() > 1,
        "the chip is a run of cells and not a lone glyph: {ink:?}"
    );
    assert!(
        ink.windows(2).all(|pair| pair[0] == pair[1]),
        "the chip is one ink from its glyph to its last cell: {ink:?}"
    );
    ink[0]
}

/// The cell the frame gives the cursor, as the terminal is told it: a glyph two
/// cells wide moves the cursor two columns while the draft's own index moves
/// one.
pub fn cursor(app: &mut App, width: u16, height: u16) -> Position {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("a test terminal");
    terminal
        .draw(|frame| render::draw(app, frame))
        .expect("the frame draws");
    terminal
        .get_cursor_position()
        .expect("the frame places the cursor")
}

/// A key pressed, with the modifiers a shortcut of the frame's needs.
pub fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.key(KeyEvent::new(code, modifiers));
}

/// Text typed one character at a time, as a keyboard sends it.
pub fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

/// The rows as one string, for an assertion that names a phrase.
pub fn shown(rows: &[String]) -> String {
    rows.join("\n")
}

/// Whether any row says `needle`.
pub fn any(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

/// The first row that says `needle`.
pub fn row_of(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row says {needle:?}: {rows:#?}"))
}

/// The first row that says `needle`, or nothing at all where none does.
pub fn row(rows: &[String], needle: &str) -> String {
    rows.iter()
        .find(|row| row.contains(needle))
        .cloned()
        .unwrap_or_default()
}

/// The badge, as the footer says it.
pub fn badge(rows: &[String]) -> Option<String> {
    rows.iter()
        .find_map(|row| row.find('↓').map(|at| row[at..].trim_end().to_string()))
}

/// The drawings the frames so far have asked for, made here as the loop makes
/// them off the frame's own thread: the store asks, the drawing is made, and the
/// answer is handed back. A test has no loop, so it is the loop — and a diagram
/// takes two frames to see, the one that stands the panel up and the one after
/// the answer.
pub fn drained(app: &mut App) {
    for body in app.take_drawings() {
        let drawing = sindri_tui::picture::draw(&body);
        app.drawn(body, drawing);
    }
}

/// What the reader's window is showing, without drawing it again.
pub fn reading(app: &mut App) -> Vec<String> {
    app.viewport().rows.clone()
}

/// One turn as the loop runs it: the prompt is sent, an answer streams, a
/// call is announced with a line of output, and the run ends.
pub fn turn(app: &mut App, prompt: &str, turn: u32, command: &str) {
    type_text(app, prompt);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    app.start().expect("a prompt starts a run");
    app.event(Event::TextDelta {
        turn,
        text: format!("answer {turn}"),
    });
    app.event(Event::ToolCall {
        turn,
        command: Some(command.to_string()),
    });
    app.output(Side::Stdout, "out 1");
    app.finished(Finished {
        stopped: Stop::Final,
        turns: turn,
        ..Finished::default()
    });
}

/// A finished call that ran `lines` lines of output, collapsed to its own row.
pub fn call(app: &mut App, turn: u32, lines: usize) {
    app.event(Event::ToolCall {
        turn,
        command: Some("cargo test".to_string()),
    });
    for line in 1..=lines {
        app.output(Side::Stdout, &format!("line {line}"));
    }
    app.over(CallOver::Exited(0), Spilled::default());
}

/// Every note the frame has taken, oldest first, as one string.
pub fn notes(app: &App) -> String {
    app.entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Note(note) => Some(note.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
