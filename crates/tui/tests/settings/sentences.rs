//! What a run says when it has nothing to send with: three sentences — the
//! transcript's line when the frame opens, the composer's line where a prompt
//! would be, and the one line a prompt typed anyway gets — over the three states
//! of absence, pinned word for word. They are built from one description of an
//! absence, so a fourth state must not be able to change what a reader sees.

use sindri_core::config::Provider;
use sindri_tui::settings::Missing;

/// A run speaking Anthropic, absent in this row or these rows.
fn absent(key: bool, model: bool) -> Missing {
    Missing::of(Provider::Anthropic, key, model).expect("a run missing at least one row")
}

#[test]
fn the_transcript_says_what_is_absent_and_every_variable_that_sets_it() {
    for (missing, wanted) in [
        (
            absent(true, false),
            "no key: type one in the settings modal, or set SINDRI_API_KEY or ANTHROPIC_API_KEY",
        ),
        (
            absent(false, true),
            "no model: type one in the settings modal, or set SINDRI_MODEL",
        ),
        (
            absent(true, true),
            "no key and no model: type them in the settings modal, or set SINDRI_API_KEY or ANTHROPIC_API_KEY and SINDRI_MODEL",
        ),
    ] {
        assert_eq!(missing.said(), wanted);
    }
}

#[test]
fn the_composer_says_what_is_absent_and_where_settings_fixes_it() {
    for (missing, wanted) in [
        (
            absent(true, false),
            "no key · a prompt cannot be sent · /settings, or set SINDRI_API_KEY or ANTHROPIC_API_KEY",
        ),
        (
            absent(false, true),
            "no model · a prompt cannot be sent · /settings, or set SINDRI_MODEL",
        ),
        (
            absent(true, true),
            "no key and no model · a prompt cannot be sent · /settings, or set SINDRI_API_KEY or ANTHROPIC_API_KEY",
        ),
    ] {
        assert_eq!(missing.composer(), wanted);
    }
}

#[test]
fn a_prompt_sent_anyway_says_which_row_settings_answers() {
    for (missing, wanted) in [
        (
            absent(true, false),
            "not sent: this run has no key — /settings saves one",
        ),
        (
            absent(false, true),
            "not sent: this run has no model — /settings names one",
        ),
        (
            absent(true, true),
            "not sent: this run has no key and no model — /settings sets both",
        ),
    ] {
        assert_eq!(missing.refused(), wanted);
    }
}

#[test]
fn a_sentence_about_a_key_names_the_variable_of_the_wire_the_run_speaks() {
    let missing = Missing::of(Provider::Openai, true, false).expect("a run with no key");

    let said = missing.said();
    assert!(
        said.contains("OPENAI_API_KEY") && !said.contains("ANTHROPIC_API_KEY"),
        "an OpenAI run names its own variable: {said}"
    );
}
