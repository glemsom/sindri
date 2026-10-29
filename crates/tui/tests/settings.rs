//! `/settings`: the modal over the one config file, through the one schema
//! (issue #18, ADR-0014). Every test points the frame at a temporary config
//! path, so nothing here can read or write a real user's config — and every
//! test that claims "nothing is written yet" says it about a file that does not
//! exist.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sindri_tui::entry::Entry;
use sindri_tui::render;
use sindri_tui::{App, Launch};
use tempfile::TempDir;

/// A frame pointed at a temporary config file, which holds `file` when given.
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

/// What is in the config file now.
fn written(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.key(KeyEvent::new(code, modifiers));
}

/// Types a run of characters, as a keyboard would.
fn type_text(app: &mut App, text: &str) {
    for ch in text.chars() {
        press(app, KeyCode::Char(ch), KeyModifiers::NONE);
    }
}

fn down(app: &mut App) {
    press(app, KeyCode::Down, KeyModifiers::NONE);
}

/// Opens `/settings` from the composer, the way a user does.
fn open(app: &mut App) {
    type_text(app, "/settings");
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

/// The rows of the frame, as a terminal would show them.
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

/// The row on screen that names one key of the schema.
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
    // A model, a turn cap and a secret: everything a real edit would touch.
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

    // The draft is gone: what the modal shows now is what the file says.
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
    // provider, model, base_url, api_key, then max_turns.
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

#[test]
fn the_api_key_defaults_to_the_environment_and_a_typed_one_warns_in_plaintext() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        row(screen(&mut f.app, 100, 30).as_slice(), "api_key").contains("from environment"),
        "the field starts at the environment, not at a secret: {rows}"
    );

    // base_url, then api_key.
    down(&mut f.app);
    down(&mut f.app);
    down(&mut f.app);
    assert!(
        !shown(&screen(&mut f.app, 100, 30)).contains("plaintext"),
        "no warning until a key is actually typed"
    );
    type_text(&mut f.app, "sk-never-on-screen");
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("sk-never-on-screen"),
        "the value is masked, never echoed: {rows}"
    );
    assert!(
        rows.contains("plaintext") && rows.contains("config.toml"),
        "and the warning names the path it would be written to: {rows}"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let file = written(&f.path);
    assert!(
        file.contains("sk-never-on-screen"),
        "saving is the explicit act that writes it: {file}"
    );
    assert!(
        !notes(&f.app).contains("sk-never-on-screen"),
        "the transcript never holds the key: {}",
        notes(&f.app)
    );
    assert!(
        !shown(&screen(&mut f.app, 100, 30)).contains("sk-never-on-screen"),
        "and neither does the frame after it closes"
    );
}

#[test]
fn a_key_already_in_the_file_is_said_to_be_there_and_never_shown() {
    let mut f = fixture(Some(
        "model = \"m\"\napi_key = \"sk-already-in-the-file\"\n",
    ));
    open(&mut f.app);
    let rows = screen(&mut f.app, 100, 30);
    assert!(
        row(&rows, "api_key").contains("set in the file"),
        "the row says a key is there: {:?}",
        row(&rows, "api_key")
    );
    assert!(
        !shown(&rows).contains("sk-already-in-the-file"),
        "and never says which: {}",
        shown(&rows)
    );
}

#[test]
fn the_thinking_toggle_is_a_keystroke_and_lands_in_the_file() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    for _ in 0..6 {
        down(&mut f.app);
    }
    assert!(
        row(&screen(&mut f.app, 100, 30), "thinking").contains("off"),
        "the file said nothing, so the toggle shows the run's default"
    );
    press(&mut f.app, KeyCode::Char(' '), KeyModifiers::NONE);
    assert!(
        row(&screen(&mut f.app, 100, 30), "thinking").contains("on"),
        "space flips it"
    );
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("thinking = true"),
        "and saving writes it: {}",
        written(&f.path)
    );
}

#[test]
fn the_extra_writable_paths_are_a_field_the_run_would_accept() {
    let mut f = fixture(Some("model = \"m\"\n"));
    open(&mut f.app);
    for _ in 0..7 {
        down(&mut f.app);
    }
    type_text(&mut f.app, "/usr/share");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        written(&f.path).contains("/usr/share"),
        "the path is in the file: {}",
        written(&f.path)
    );
}

#[test]
fn an_unknown_key_in_the_file_refuses_the_modal_and_says_why() {
    let mut f = fixture(Some("model = \"m\"\nmax_turn = 5\n"));
    open(&mut f.app);

    assert!(
        f.app.overlay().is_none(),
        "no modal opens over a broken file"
    );
    let said = notes(&f.app);
    assert!(
        said.contains("max_turn") && said.contains(&f.path.display().to_string()),
        "the transcript names the key and the file: {said}"
    );
    assert_eq!(
        written(&f.path),
        "model = \"m\"\nmax_turn = 5\n",
        "and nothing was rewritten"
    );
}

#[test]
fn the_modal_fits_a_hundred_columns_and_a_hundred_and_sixty() {
    let mut f = fixture(Some("model = \"qwen3-32b\"\napi_key = \"sk-hidden\"\n"));
    open(&mut f.app);
    // provider, model, base_url, then the key itself.
    for _ in 0..3 {
        down(&mut f.app);
    }
    type_text(&mut f.app, "sk-typed-secret");
    for width in [100u16, 160] {
        let rows = screen(&mut f.app, width, 30);
        let text = shown(&rows);
        for wanted in [
            "settings",
            "provider",
            "model",
            "api_key",
            "thinking",
            "extra_headers",
            "plaintext",
            "Enter save",
            "Esc discard",
        ] {
            assert!(
                text.contains(wanted),
                "{width} columns shows {wanted}: {text}"
            );
        }
        assert!(
            !text.contains("sk-typed-secret") && !text.contains("sk-hidden"),
            "{width} columns never shows a secret: {text}"
        );
    }
}
