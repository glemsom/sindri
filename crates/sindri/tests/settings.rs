//! The settings modal where it meets the file: a visit that writes only the rows a reader
//! touched, `Enter` writing at once and the next prompt running on what it left, `Esc`
//! writing nothing, an emptied row unsetting its key so the environment speaks again, and a
//! value the schema refuses said on the screen with nothing written at all (§12, §11,
//! ADR-0009).
//!
//! The frame is driven through the keys a reader presses — the mode stack's own tables, the
//! screen's own map — and the write goes through `settings::persist`, which is the very
//! function the terminal calls. So every promise here is the product's and none of it is a
//! test-only path.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sindri::composer::{Key, Outcome};
use sindri::keys::Mode;
use sindri::settings::{self, Settings};
use sindri::view::View;
use sindri_core::config::{Config, Flags, MODE, Touched};
use tempfile::TempDir;

/// A config file a person wrote: comments above keys and beside them, keys in an order no
/// serializer would choose, and a blank line where they wanted one.
const HAND_WRITTEN: &str = "\
# My settings, and my notes on them. The comments are the point.
status = false   # OSC 7501 reporting, off until I trust it

# The gateway moved last spring; this is where it went.
base_url = \"https://gateway.test/v1\"
# swapped a few times a week
model = \"old-model\"  # note beside it
max_turns = 250

thinking = true
";

/// A config home holding `text` as its one `config.toml`, the way a person left it.
fn home(text: &str) -> TempDir {
    let home = tempfile::tempdir().expect("a temp dir");
    let directory = home.path().join("sindri");
    std::fs::create_dir_all(&directory).expect("the sindri directory");
    std::fs::write(directory.join("config.toml"), text).expect("the config file");
    home
}

/// The one file inside a config home.
fn file_in(home: &TempDir) -> PathBuf {
    home.path().join("sindri").join("config.toml")
}

/// What the file holds now, read the way any other reader would read it.
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("a readable config")
}

/// The environment a launch resolves against: this config home, and nothing else set.
fn environment(home: &TempDir) -> BTreeMap<String, String> {
    [(
        String::from("XDG_CONFIG_HOME"),
        home.path().display().to_string(),
    )]
    .into_iter()
    .collect()
}

/// The config the visit opened on: the file under the environment, resolved the way a
/// launch resolves it — which is what the screen shows and what the next prompt runs on.
fn resolved(home: &TempDir) -> Config {
    Config::resolve(&environment(home), Flags::default()).expect("a config that resolves")
}

/// The frame a reader opens the screen on: the config as it resolved, and the screen over
/// the draft.
fn opened(home: &TempDir) -> View {
    let mut view = View::boot();
    view.open_settings(&resolved(home));
    view
}

/// The screen walked until `key`'s row is the one a key acts on, as a reader walks it.
fn walk_to(view: &mut View, key: &str) {
    for _ in 0..Config::KEYS.len() {
        if row_key(view.settings()) == Some(key) {
            return;
        }
        view.key(Key::Down);
    }
    panic!("{key} is a key of the schema, so the screen has a row for it");
}

/// The key the screen's cursor is on, as the screen paints it.
fn row_key(settings: &Settings) -> Option<&'static str> {
    settings.rows().nth(settings.current()).map(|(key, _)| key)
}

/// A row's text as the screen holds it.
fn shown(view: &View, key: &str) -> String {
    view.settings()
        .rows()
        .find(|(name, _)| *name == key)
        .map(|(_, text)| text.to_owned())
        .unwrap_or_else(|| panic!("the screen has a row for {key}"))
}

/// The text a reader types into the row the keys act on, one glyph at a time.
fn type_into(view: &mut View, text: &str) {
    for glyph in text.chars() {
        view.key(Key::Char(glyph));
    }
}

/// The patch `Enter` handed the terminal, or a panic saying the key did not ask for a write.
fn entered(view: &mut View) -> Touched {
    match view.key(Key::Enter) {
        Outcome::Save(touched) => touched,
        other => panic!("Enter should have asked for a write, and {other:?} came instead"),
    }
}

#[test]
fn enter_writes_the_one_row_a_reader_touched_and_every_other_byte_survives() {
    let home = home(HAND_WRITTEN);
    let path = file_in(&home);
    let mut view = opened(&home);
    walk_to(&mut view, "model");
    view.key(Key::Clear);
    type_into(&mut view, "new-model");

    let touched = entered(&mut view);
    settings::persist(&environment(&home), &touched, Flags::default()).expect("the write lands");
    view.saved();

    assert_eq!(
        read(&path),
        HAND_WRITTEN.replace("model = \"old-model\"", "model = \"new-model\""),
        "a visit that wrote into one row writes one line: the comments, the blank line and \
         every key nobody reached are the bytes they were"
    );
    assert_eq!(
        std::fs::metadata(&path)
            .expect("a file to stat")
            .permissions()
            .mode()
            & 0o777,
        MODE,
        "one of the keys a visit can write is a secret, so the file is owner-only"
    );
    assert_eq!(
        view.mode(),
        Mode::Composer,
        "and the visit is over: Enter closes the screen it wrote from"
    );
}

#[test]
fn enter_writes_at_once_and_the_next_prompt_runs_on_the_value_it_wrote() {
    let home = home(HAND_WRITTEN);
    let mut view = opened(&home);
    walk_to(&mut view, "model");
    view.key(Key::Clear);
    type_into(&mut view, "claude-sonnet-4-5");

    let touched = entered(&mut view);
    let fresh = settings::persist(&environment(&home), &touched, Flags::default())
        .expect("the write lands");

    assert_eq!(
        fresh.model.as_deref(),
        Some("claude-sonnet-4-5"),
        "the config the next prompt is handed is resolved again from the file the visit \
         wrote, so it runs on what the visit left and not on what the launch was handed"
    );
    assert_eq!(
        resolved(&home).model.as_deref(),
        Some("claude-sonnet-4-5"),
        "and any later launch resolves to the same value"
    );
}

#[test]
fn enter_writes_once_and_a_visit_that_touched_nothing_writes_at_all() {
    let home = home(HAND_WRITTEN);
    let path = file_in(&home);
    let mut view = opened(&home);
    walk_to(&mut view, "max_turns");

    assert_eq!(
        view.key(Key::Enter),
        Outcome::Edited,
        "a visit that read the screen and wrote nothing has nothing to hand the file: Enter \
         closes, and no write is asked for"
    );
    assert_eq!(view.mode(), Mode::Composer);
    assert_eq!(read(&path), HAND_WRITTEN, "the file is the file it was");
}

#[test]
fn escape_discards_everything_a_reader_typed() {
    let home = home(HAND_WRITTEN);
    let path = file_in(&home);
    let mut view = opened(&home);
    walk_to(&mut view, "model");
    view.key(Key::Clear);
    type_into(&mut view, "a model the file never hears about");

    assert_eq!(view.key(Key::Escape), Outcome::Edited);

    assert_eq!(view.mode(), Mode::Composer, "the screen is gone");
    assert_eq!(
        read(&path),
        HAND_WRITTEN,
        "and Esc writes nothing: a visit a reader backed out of is a file nobody touched"
    );
}

#[test]
fn an_emptied_row_unsets_the_key_so_the_environment_speaks_again() {
    let home = home(HAND_WRITTEN);
    let path = file_in(&home);
    let mut environment = environment(&home);
    environment.insert(
        String::from("SINDRI_MODEL"),
        String::from("from-the-environment"),
    );
    let mut view = View::boot();
    view.open_settings(&Config::resolve(&environment, Flags::default()).expect("a config"));
    walk_to(&mut view, "model");

    view.key(Key::Clear);
    let touched = entered(&mut view);
    let fresh = settings::persist(&environment, &touched, Flags::default()).expect("the write");

    let after = read(&path);
    assert!(
        !after.contains("model"),
        "Ctrl-U empties the row and the save unsets the key: the file is:\n{after}"
    );
    assert_eq!(
        fresh.model.as_deref(),
        Some("from-the-environment"),
        "and the environment, which was under the file, is what speaks for the key now"
    );
}

#[test]
fn a_bad_value_is_refused_on_the_screen_naming_its_key_and_nothing_is_written() {
    let home = home(HAND_WRITTEN);
    let path = file_in(&home);
    let mut view = opened(&home);
    walk_to(&mut view, "max_turns");
    view.key(Key::Clear);
    type_into(&mut view, "two hundred");

    let touched = entered(&mut view);
    let error = settings::persist(&environment(&home), &touched, Flags::default())
        .expect_err("a turn cap that is not a number is a bad value");
    view.refuse(&error.to_string());

    assert_eq!(
        view.mode(),
        Mode::Settings,
        "a refusal is read where the value is, so the screen stays open to be fixed"
    );
    assert!(
        view.settings()
            .refusal()
            .is_some_and(|reason| reason.contains("max_turns")),
        "and it names the key a reader has to go and change, and it says: {error}"
    );
    assert_eq!(
        read(&path),
        HAND_WRITTEN,
        "a refused save writes nothing at all, not even the rows that would have parsed"
    );
}

#[test]
fn a_tab_walked_row_is_the_row_a_visit_writes() {
    let home = home(HAND_WRITTEN);
    let path = file_in(&home);
    let mut view = opened(&home);

    view.key(Key::Tab);
    view.key(Key::Tab);
    type_into(&mut view, "chat");

    assert_eq!(
        row_key(view.settings()),
        Some("route"),
        "two Tabs are two keys down the schema, which is the second way §12 gives a reader \
         of walking the list"
    );
    let touched = entered(&mut view);
    settings::persist(&environment(&home), &touched, Flags::default()).expect("the write");

    assert_eq!(
        read(&path),
        format!("{HAND_WRITTEN}route = \"chat\"\n"),
        "the key the reader walked to is the key the file gains, appended after the ones it \
         already had"
    );
}

#[test]
fn space_flips_a_toggle_and_the_file_holds_the_word_it_was_flipped_to() {
    let home = home(HAND_WRITTEN);
    let path = file_in(&home);
    let mut view = opened(&home);
    walk_to(&mut view, "thinking");

    view.key(Key::Char(' '));
    let touched = entered(&mut view);
    settings::persist(&environment(&home), &touched, Flags::default()).expect("the write");

    assert_eq!(
        read(&path),
        HAND_WRITTEN.replace("thinking = true", "thinking = false"),
        "Space is a switch on the three keys that hold one, and the line it flips is the \
         only line that moves"
    );
}

#[test]
fn the_screen_starts_from_the_config_as_it_resolved_and_not_from_the_file_alone() {
    let home = home(HAND_WRITTEN);
    let mut environment = environment(&home);
    environment.insert(
        String::from("SINDRI_MODEL"),
        String::from("from-the-environment"),
    );
    let mut view = View::boot();
    view.open_settings(&Config::resolve(&environment, Flags::default()).expect("a config"));
    walk_to(&mut view, "thinking");

    assert_eq!(
        shown(&view, "model"),
        "from-the-environment",
        "the environment is read above the file, so the row that shows the model shows what \
         a run would use"
    );

    view.key(Key::Char(' '));
    let touched = entered(&mut view);
    assert_eq!(
        touched.rows().collect::<Vec<_>>(),
        vec![("thinking", "false")],
        "and a row nobody wrote into is never written: the model the environment speaks \
         stays out of the file, however plainly the screen was showing it"
    );
}
