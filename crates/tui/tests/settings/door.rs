//! The door: a run with no credential opens the frame instead of exiting, with
//! the settings modal staged on the key's row. The modal says where
//! a key comes from, where it is written, that the file is `0600` and that the
//! value is never shown again; `Esc` leaves a frame that still reads, with the
//! missing key said once.

use super::*;
use sindri_core::config::{Key, Provider};
use sindri_tui::overlay::Overlay;
use sindri_tui::settings::Missing;

/// A first run: no config file, and no credential for this run.
fn first_run() -> Fixture {
    door(Missing::of(Provider::Anthropic, true, false).expect("a run with no key"))
}

/// The frame the front door opens when the run has nothing for these rows.
fn door(missing: Missing) -> Fixture {
    let dir = TempDir::new().expect("a temp directory");
    let path = dir.path().join("sindri/config.toml");
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
            context_limit: None,
        },
        None,
    )
    .with_config(&path)
    .with_missing(missing);
    Fixture {
        _dir: dir,
        path,
        app,
    }
}

/// The modal the frame opened on, and the row its cursor is in.
fn staged(app: &App) -> (Key, String) {
    let Some(Overlay::Settings(settings)) = app.overlay() else {
        panic!("the frame opens on the settings modal");
    };
    let field = settings
        .fields()
        .get(settings.chosen())
        .expect("the modal has a row with the cursor");
    (field.key, field.key.name().to_string())
}

fn mentions(app: &App, phrase: &str) -> usize {
    notes(app).matches(phrase).count()
}

/// The rows as one flowing line: a long path wraps, and a wrapper's break is
/// a fact about the width, not about what the modal says.
fn flowing(rows: &[String]) -> String {
    rows.join("")
}

#[test]
fn a_run_with_no_key_opens_the_modal_on_the_key_row_and_says_what_that_row_needs() {
    let mut f = first_run();

    let (key, row) = staged(&f.app);
    assert_eq!(key, Key::ApiKey, "the modal opens on the key's row: {row}");

    let rows = screen(&mut f.app, 100, 36);
    let text = flowing(&rows);
    for wanted in [
        "no credential",
        "SINDRI_API_KEY",
        "ANTHROPIC_API_KEY",
        "0600",
        "never shown again",
    ] {
        assert!(
            text.contains(wanted),
            "the modal says {wanted:?} on the row it opens on: {text}"
        );
    }
    // The path itself is longer than a row in this sandbox, so the modal wraps
    // it: what is asserted is that the row names the file it writes, and the
    // header above the rows names it in full.
    assert!(
        text.contains("written to") && text.contains("config.toml"),
        "and says where the key would be written: {text}"
    );
}

#[test]
fn esc_leaves_a_frame_that_reads_and_the_missing_key_is_said_once() {
    let mut f = first_run();
    assert_eq!(
        mentions(&f.app, "no key"),
        1,
        "the transcript says the key is missing, once: {}",
        notes(&f.app)
    );

    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(f.app.overlay().is_none(), "Esc closes the modal");
    assert!(!f.path.exists(), "and writes nothing");
    assert_eq!(
        mentions(&f.app, "no key"),
        1,
        "Esc does not say it again: {}",
        notes(&f.app)
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("a prompt cannot be sent"),
        "the composer states what it cannot do: {rows}"
    );
    assert!(
        rows.contains("/settings"),
        "and names the way to fix it: {rows}"
    );

    // The frame still reads: `?` opens the bindings over it, as on any run.
    press(&mut f.app, KeyCode::Char('?'), KeyModifiers::NONE);
    assert!(
        shown(&screen(&mut f.app, 100, 30)).contains("dismiss a completion or an overlay"),
        "the frame is usable for reading and browsing after Esc"
    );
}

#[test]
fn a_prompt_is_not_sent_while_the_run_has_no_key() {
    let mut f = first_run();
    press(&mut f.app, KeyCode::Esc, KeyModifiers::NONE);
    type_text(&mut f.app, "what changed?");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(
        f.app.start().is_none(),
        "nothing is sent while the run has no key"
    );
}

#[test]
fn a_key_typed_into_the_staged_modal_saves_masked_and_the_composer_is_a_prompt_again() {
    let mut f = first_run();
    type_text(&mut f.app, "sk-typed-on-the-first-run");

    let rows = shown(&screen(&mut f.app, 100, 36));
    assert!(
        !rows.contains("sk-typed-on-the-first-run"),
        "the key is masked while it is typed: {rows}"
    );

    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(f.app.overlay().is_none(), "saving closes the modal");
    let file = written(&f.path);
    assert!(
        file.contains("sk-typed-on-the-first-run"),
        "and writes it to the file it named: {file}"
    );
    assert!(
        f.app.take_reload(),
        "the next prompt runs on it, with no restart"
    );
    assert!(
        !notes(&f.app).contains("sk-typed-on-the-first-run"),
        "the key is never transcribed: {}",
        notes(&f.app)
    );
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("a prompt cannot be sent") && !rows.contains("no credential"),
        "the composer is a prompt again: {rows}"
    );
}

#[test]
fn a_run_with_nothing_configured_says_both_once_and_still_opens_on_the_key() {
    let mut f = door(Missing::of(Provider::Anthropic, true, true).expect("a run with nothing"));

    assert_eq!(
        mentions(&f.app, "no key and no model"),
        1,
        "the transcript says both are missing, once: {}",
        notes(&f.app)
    );
    assert_eq!(
        staged(&f.app).0,
        Key::ApiKey,
        "and the cursor is on the key"
    );

    // Editing the key of a run that has no model leaves the model missing: the
    // composer still says so rather than dispatching a prompt nobody can answer.
    type_text(&mut f.app, "sk-typed-on-the-first-run");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        rows.contains("no model") && rows.contains("a prompt cannot be sent"),
        "the row that is still absent is still said: {rows}"
    );
    assert!(
        !rows.contains("sk-typed-on-the-first-run"),
        "and the key is never drawn: {rows}"
    );
}

#[test]
fn a_save_that_fills_the_only_absent_row_leaves_a_composer_that_can_send() {
    // A credential in the environment and no model: the modal opens on the
    // model, and saving one answers for the run — nothing keeps saying a value
    // is missing that the file now carries.
    let mut f = door(Missing::of(Provider::Anthropic, false, true).expect("a run with no model"));
    assert_eq!(
        staged(&f.app).0,
        Key::Model,
        "the cursor is on the row it has nothing for"
    );
    assert_eq!(
        mentions(&f.app, "no model"),
        1,
        "the transcript says it: {}",
        notes(&f.app)
    );

    type_text(&mut f.app, "claude-sonnet-4-6");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(f.app.take_reload(), "the next prompt runs on it");
    // The transcript keeps the line it was given; what stops is the composer
    // saying the run cannot send — and the refusal that would follow.
    let rows = shown(&screen(&mut f.app, 100, 30));
    assert!(
        !rows.contains("a prompt cannot be sent"),
        "the composer no longer says the run is missing a value: {rows}"
    );
    type_text(&mut f.app, "what changed?");
    press(&mut f.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(f.app.start().is_some(), "and a prompt is dispatched again");
}
