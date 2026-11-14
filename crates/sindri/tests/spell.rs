//! A call's arguments are a spelling of a tool's own fact: a command is a command line, a path is a
//! path, a skill is a name. What a row says is the thing the reader would type, and the model's
//! braces never reach the frame.

use sindri::spell;
use sindri_core::tools::{BASH, OPEN, SKILL};

#[test]
fn a_bash_call_draws_the_command_line_a_reader_would_type() {
    assert_eq!(
        spell::of(BASH, r#"{"command": "rg -n 'fence' src"}"#),
        "rg -n 'fence' src",
        "a command's own quotes are the reader's words, kept whole and never JSON's"
    );
}

#[test]
fn a_command_keeps_the_newlines_and_the_indent_it_was_written_with() {
    assert_eq!(
        spell::of(BASH, "{\"command\": \"cargo test\\n    && make check\"}"),
        "cargo test\n    && make check",
        "a command's own lines are the reader's own lines"
    );
}

#[test]
fn a_skill_call_draws_the_skills_own_name() {
    assert_eq!(
        spell::of(SKILL, r#"{"name": "rust-review"}"#),
        "rust-review"
    );
}

#[test]
fn an_open_call_draws_the_path_it_names_and_a_url_when_it_names_no_path() {
    assert_eq!(
        spell::of(OPEN, r#"{"path": "crates/sindri/src/rows.rs"}"#),
        "crates/sindri/src/rows.rs"
    );
    assert_eq!(
        spell::of(OPEN, r#"{"url": "https://example.test/a b"}"#),
        "https://example.test/a b",
        "a url is one word to the model that sent it, spaces and all"
    );
}

#[test]
fn the_words_a_model_sent_instead_of_arguments_are_drawn_as_they_were_sent() {
    assert_eq!(spell::of(BASH, "ls -la"), "ls -la");
}

#[test]
fn an_argument_no_tool_spells_is_drawn_as_words_and_never_as_punctuation() {
    for arguments in [
        r#"{"glob": "*.rs", "pattern": "fn main"}"#,
        r#"{"all": true, "depth": 3, "paths": ["a.rs", {"deep": null}]}"#,
        r#"{"flags": ["-n", "-E"], "limit": 12}"#,
    ] {
        let drawn = spell::of("grep", arguments);
        assert!(
            !drawn.contains(['{', '}', '[', ']', '"', ':', ',']),
            "{drawn} still wears the punctuation of {arguments}"
        );
    }

    assert_eq!(
        spell::of("grep", r#"{"glob": "*.rs", "pattern": "fn main"}"#),
        "glob *.rs pattern fn main",
        "a call this frame knows no tool for says each key and the value under it"
    );
    assert_eq!(
        spell::of(BASH, r#"{"cmd": "ls"}"#),
        "cmd ls",
        "and a tool this product has, whose own key is missing, is spelled the same way"
    );
    assert_eq!(
        spell::of(
            "grep",
            r#"{"all": true, "depth": 3, "paths": ["a.rs", {"deep": null}]}"#
        ),
        "all true depth 3 paths a.rs deep",
        "an array's items and an object's keys are words like any other, and a null says nothing"
    );
}

#[test]
fn arguments_that_spell_nothing_draw_nothing() {
    assert_eq!(spell::of(BASH, ""), "");
}
