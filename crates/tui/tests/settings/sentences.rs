//! What a run says when it has nothing to send with: three sentences — the
//! transcript's line when the frame opens, the composer's line where a prompt
//! would be, and the one line a prompt typed anyway gets — over the three states
//! of absence, pinned word for word. They are built from one description of an
//! absence, so a fourth state must not be able to change what a reader sees.

use sindri_core::config::Provider;
use sindri_tui::settings::Missing;

/// A run speaking Anthropic that has no key, or no model, or neither.
fn no_key() -> Missing {
    Missing::of(Provider::Anthropic, true, false).expect("a run with no key")
}

fn no_model() -> Missing {
    Missing::of(Provider::Anthropic, false, true).expect("a run with no model")
}

fn neither() -> Missing {
    Missing::of(Provider::Anthropic, true, true).expect("a run with no key and no model")
}

#[test]
fn the_transcript_says_what_is_absent_and_every_variable_that_sets_it() {
    for (missing, wanted) in [
        (
            no_key(),
            "no key: type one in the settings modal, or set SINDRI_API_KEY or ANTHROPIC_API_KEY",
        ),
        (
            no_model(),
            "no model: type one in the settings modal, or set SINDRI_MODEL",
        ),
        (
            neither(),
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
            no_key(),
            "no key · a prompt cannot be sent · /settings, or set SINDRI_API_KEY or ANTHROPIC_API_KEY",
        ),
        (
            no_model(),
            "no model · a prompt cannot be sent · /settings, or set SINDRI_MODEL",
        ),
        (
            neither(),
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
            no_key(),
            "not sent: this run has no key — /settings saves one",
        ),
        (
            no_model(),
            "not sent: this run has no model — /settings names one",
        ),
        (
            neither(),
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
