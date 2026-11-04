//! `/model`: the names the wire the run speaks offered, on a screen of their
//! own. The screen is a choice and not a field — every name on it is one the
//! wire named — so a fragment typed on it narrows the names by the ranking the
//! model row already uses, `Enter` writes the name the cursor is on to the
//! config file the run re-reads without a restart, and `Esc` closes the screen
//! with the file untouched. A wire that named nothing still opens it, and says
//! where the names would be why there are none. Every fixture points the frame
//! at a temporary config path, so nothing here reads or writes a real user's
//! file.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

#[path = "../common/mod.rs"]
mod common;

pub use common::{launch, notes, press, row_of, screen, shown, type_text};
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyModifiers};
use sindri_tui::App;
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
    let app = App::new(common::launch(), None).with_config(&path);
    Fixture {
        _dir: dir,
        path,
        app,
    }
}

fn written(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// `/model`, typed and sent: the name opens the offer the catalogue makes, and
/// the first `Enter` takes that offer while the second sends the command.
fn open(app: &mut App) {
    type_text(app, "/model");
    press(app, KeyCode::Enter, KeyModifiers::NONE);
    press(app, KeyCode::Enter, KeyModifiers::NONE);
}

/// The screen open on a listing that has landed: what every case here starts
/// from, so a test says only what it does differently.
fn offered(file: Option<&str>, names: &[&str]) -> Fixture {
    let mut f = fixture(file);
    open(&mut f.app);
    let serving = f
        .app
        .take_ask()
        .expect("opening the screen asks the wire for its names");
    f.app.listed(
        serving,
        Some(names.iter().map(|name| name.to_string()).collect()),
    );
    f
}

/// The name the screen's cursor is on: the row it marks.
fn marked(f: &mut Fixture) -> String {
    let rows = screen(&mut f.app, 100, 30);
    let row = rows
        .iter()
        .find(|row| row.contains("▸ "))
        .unwrap_or_else(|| panic!("the screen marks one name: {}", shown(&rows)));
    row.split("▸ ")
        .nth(1)
        .and_then(|after| after.split_whitespace().next())
        .expect("the mark has a name after it")
        .to_string()
}

mod names;
mod write;
