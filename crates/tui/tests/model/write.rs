//! What the screen writes: `Enter` takes the name the cursor is on into the
//! config file through the document the modal saves, and the frame is told to
//! read the file again so the next prompt runs on it without a restart.
//! `/model <name>` is the same write for a name the wire never offered, and
//! `Esc` is the one key that writes nothing.

use super::*;

use sindri_core::config::{Document, Key};
use sindri_tui::Launch;

#[test]
fn enter_writes_the_name_the_cursor_is_on_and_the_next_prompt_reads_it() {
    let mut f = offered(Some("model = \"gpt-5\"\n"), &["gpt-5", "qwen3-32b"]);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(f.app.overlay().is_none(), "taking a name closes the screen");
    let file = written(&f.path);
    assert!(
        file.contains("model = \"qwen3-32b\""),
        "the file holds the name the cursor was on: {file}"
    );
    assert!(
        !file.contains("gpt-5"),
        "and not the one it replaced: {file}"
    );

    // The shape the run re-reads, read back the way a boot reads it rather than
    // guessed at from the file's text.
    let reread = Document::load_at(&f.path).expect("the file a run reads");
    assert_eq!(
        reread.editable(Key::Model),
        "qwen3-32b",
        "the name is a model a boot reads back"
    );
    assert!(
        f.app.take_reload(),
        "and the front end is asked to read the file again, so no restart is needed"
    );

    f.app.reloaded(Ok(Launch {
        model: reread.editable(Key::Model),
        ..common::launch()
    }));
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("qwen3-32b"),
        "the frame states the name the next prompt runs on: {rows}"
    );
    let said = notes(&f.app);
    assert!(
        said.contains("model set to qwen3-32b") && said.contains(&f.path.display().to_string()),
        "and the transcript says which name and which file: {said}"
    );
}

#[test]
fn model_with_a_name_sets_a_name_the_wire_never_offered() {
    let mut f = fixture(Some("model = \"gpt-5\"\n"));
    type_text(&mut f.app, "/model qwen3-coder");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(
        f.app.overlay().is_none(),
        "a name on the line is a name to set, not a screen to open"
    );
    let file = written(&f.path);
    assert!(
        file.contains("model = \"qwen3-coder\""),
        "the name the wire never offered is the file's: {file}"
    );
    let reread = Document::load_at(&f.path).expect("the file a run reads");
    assert_eq!(
        reread.editable(Key::Model),
        "qwen3-coder",
        "written in the shape a boot reads back"
    );
    assert!(
        f.app.take_reload(),
        "and the next prompt is told to read it, exactly as the screen's own is"
    );
}

#[test]
fn esc_closes_the_screen_and_leaves_the_file_exactly_as_it_was() {
    let mut f = offered(Some("model = \"gpt-5\"\n"), &["gpt-5", "qwen3-32b"]);
    press(&mut f.app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(f.app.overlay().is_none(), "Esc closes the screen");
    assert_eq!(
        written(&f.path),
        "model = \"gpt-5\"\n",
        "and changes nothing in the file"
    );
    assert!(
        !f.app.take_reload(),
        "and asks the run to read nothing it did not write"
    );
}
