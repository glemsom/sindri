//! `/settings`: the modal over the one config file, through the one schema
//! (issue #18, ADR-0014). Every test points the frame at a temporary
//! config path, so nothing here can read or write a real user's config —
//! and every test that claims "nothing is written yet" says it about a
//! file that does not exist.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use sindri_tui::entry::Entry;
use sindri_tui::render;
use sindri_tui::{App, Launch};
use tempfile::TempDir;

struct Fixture {
    _dir: TempDir,
    path: PathBuf,
    app: App,
}

fn fixture(file: Option<&str>) -> Fixture {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("sindri/config.toml");
    if let Some(file) = file {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the config dir");
        std::fs::write(&path, file).expect("the config file");
    }
    let app = App::new(
        Launch {
            session: "4f1c8f2e-8a24-4b93-9c0e-0d3f0e5c1a77".to_string(),
            temp: "/state/sindri/sessions/4f1c8f2e/tmp".to_string(),
            branch: Some("main".to_string()),
            sandbox: "bwrap · 1 writable".to_string(),
            workspace: "~/git/sindri".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            max_turns: 250,
            context_limit: Some(200_000),
        },
        None,
    )
    .with_config(&path);
    Fixture {
        _dir: dir,
        path,
        app,
    }
}

fn written(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.key(KeyEvent::new(code, modifiers));
}

fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

fn down(app: &mut App) {
    press(app, KeyCode::Down, KeyModifiers::NONE);
}

/// `/settings`, typed and sent: the name opens the offer the catalogue makes,
/// the first `Enter` takes it and the second sends it (issue #54).
fn open(app: &mut App) {
    type_text(app, "/settings");
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
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

fn row(rows: &[String], key: &str) -> String {
    rows.iter()
        .find(|row| row.contains(key))
        .cloned()
        .unwrap_or_default()
}

#[test]
fn settings_opens_a_modal_over_every_key_the_file_carries() {
    let mut f = fixture(Some("model = \"qwen3-32b\"\nmax_turns = 40\n"));
    open(&mut f.app);

    assert!(f.app.overlay().is_some(), "/settings opens a modal");
    let rows = shown(&screen(&mut f.app, 100, 30));
    for key in [
        "provider",
        "model",
        "base_url",
        "api_key",
        "max_turns",
        "timeout",
        "thinking",
        "extra_writable_paths",
        "extra_headers",
    ] {
        assert!(rows.contains(key), "the modal shows {key:?}: {rows}");
    }
    assert!(
        rows.contains("file ") && f.path.display().to_string().contains("config.toml"),
        "and names the file it would write: {rows}"
    );
    assert!(
        rows.contains("sindri/config.toml"),
        "the file's path is on screen in full, not elided: {rows}"
    );
    assert!(
        rows.contains("qwen3-32b") && rows.contains("40"),
        "with the file's own values in the fields: {rows}"
    );
}

#[test]
fn esc_discards_every_edit_and_the_file_is_never_touched() {
    let mut f = fixture(None);
    open(&mut f.app);
    down(&mut f.app);
    type_text(&mut f.app, "qwen3-32b");
    down(&mut f.app);
    down(&mut f.app);
    type_text(&mut f.app, "sk-typed-and-thrown-away");

    assert!(
        !f.path.exists(),
        "nothing is written while the modal is open"
    );

    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(f.app.overlay().is_none(), "Esc closes the modal");
    assert!(!f.path.exists(), "and the config file is untouched");
    assert!(!f.app.take_reload(), "and nothing asks the run to change");

    open(&mut f.app);
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("qwen3-32b") && !rows.contains("sk-typed"),
        "a discarded edit is not a draft that came back: {rows}"
    );
}

#[test]
fn enter_writes_the_file_at_once_and_the_next_prompt_is_told_to_read_it() {
    let mut f = fixture(Some("model = \"old-model\"\n"));
    open(&mut f.app);
    down(&mut f.app);
    press(&mut f.app, KeyCode::Char('u'), KeyModifiers::CONTROL);
    type_text(&mut f.app, "qwen3-32b");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(f.app.overlay().is_none(), "saving closes the modal");
    let file = written(&f.path);
    assert!(
        file.contains("model = \"qwen3-32b\""),
        "the file holds the new value: {file}"
    );
    assert!(!file.contains("old-model"), "and not the old one: {file}");
    assert!(
        f.app.take_reload(),
        "the front end is asked to read it again, so no restart is needed"
    );
    let said = notes(&f.app);
    assert!(
        said.contains("settings saved") && said.contains(&f.path.display().to_string()),
        "and the transcript says where it went: {said}"
    );
}

#[test]
fn a_value_the_schema_refuses_keeps_the_modal_open_and_writes_nothing() {
    let mut f = fixture(Some("model = \"from-file\"\n"));
    open(&mut f.app);
    for _ in 0..4 {
        down(&mut f.app);
    }
    type_text(&mut f.app, "0");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(f.app.overlay().is_some(), "the modal stays open");
    assert!(!f.app.take_reload(), "and no reload was asked for");
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("not saved") && rows.contains("max_turns"),
        "the refusal names the field: {rows}"
    );
    assert!(
        written(&f.path).contains("from-file"),
        "the file is exactly as it was"
    );
}

mod door;
mod modal;
mod secrets;
