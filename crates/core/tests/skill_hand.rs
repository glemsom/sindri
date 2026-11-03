//! A skill a human loads by hand: `/skill:name` is the call the model would
//! make, run from the composer, so the body is in the conversation from here on
//! — and claimed in the conversation's own set, so the model's later call for it
//! is `already_loaded` rather than a second body on every turn that follows.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use std::path::Path;

use sindri_core::agent::SkillTool;
use sindri_core::provider::{Asked, Message};
use sindri_core::skills::Catalog;
use support::{Harness, answer, skills};

/// A workspace with one model-visible skill, `alpha`, holding a body.
fn alpha(workspace: &Path) -> Catalog {
    skills::write(
        workspace,
        "alpha",
        "",
        "Follow the steps.\nSee `references/api.md` for the shape of a note.",
    );
    skills::catalog(workspace)
}

#[test]
fn a_load_by_hand_puts_the_body_in_the_conversation_and_asks_for_no_turn() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let mut h = Harness::skilled(vec![answer("Done.")], Vec::new(), &catalog);

    let activated = h.activate("alpha");

    assert!(activated.loaded, "{activated:?}");
    assert!(!activated.outcome.is_error, "{activated:?}");
    assert_eq!(
        activated.outcome.facts.outcome.as_deref(),
        Some("loaded"),
        "the word the frame's row states"
    );
    assert_eq!(
        h.loaded(),
        vec!["alpha".to_string()],
        "the conversation was told it"
    );
    assert_eq!(h.events(), Vec::new(), "nothing was sent to the provider");

    // The history the next prompt is sent with: the call the model did not
    // make, and the result it will read, in the order a provider takes.
    h.run("follow the steps");
    let first = &h.conversations()[0];
    match &first[0] {
        Message::Assistant { text, calls } => {
            assert!(text.is_empty(), "a load says nothing of its own");
            assert_eq!(calls.len(), 1);
            assert_eq!(
                calls[0].asked(),
                Asked::Skill {
                    name: "alpha".to_string()
                }
            );
        }
        other => panic!("the load opens the history: {other:?}"),
    }
    let Message::ToolResults(results) = &first[1] else {
        panic!("the result follows the call: {first:?}");
    };
    assert!(
        results[0]
            .content
            .contains("<skill_content name=\"alpha\">"),
        "the body is what the conversation carries: {}",
        results[0].content
    );
    assert!(
        matches!(&first[2], Message::User(prompt) if prompt == "follow the steps"),
        "and the prompt follows it: {first:?}"
    );
}

#[test]
fn a_load_by_hand_returns_the_result_the_tool_produced() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let mut h = Harness::skilled(vec![answer("Done.")], Vec::new(), &catalog);

    let activated = h.activate("alpha");

    // The call's own identity and the facts the content was rendered from: one
    // result, made by the tool, rather than a second reading of its fields.
    assert_eq!(activated.name, "alpha");
    assert_eq!(activated.outcome.call_id, "user_skill_1");
    assert_eq!(activated.outcome.name, "skill");
    assert_eq!(
        activated.outcome.facts.outcome.as_deref(),
        Some("loaded"),
        "the word the content opens with, as a field beside it"
    );
    assert!(
        activated
            .outcome
            .content
            .starts_with("outcome: loaded\nskill: alpha\ndir: "),
        "the content is the tool's own: {}",
        activated.outcome.content
    );
}

#[test]
fn a_second_load_by_hand_is_already_loaded_and_tells_the_model_nothing_new() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let mut h = Harness::skilled(vec![answer("Done.")], Vec::new(), &catalog);

    h.activate("alpha");
    let again = h.activate("alpha");

    assert!(
        !again.loaded,
        "the set is the conversation's: once, not twice"
    );
    assert_eq!(
        again.outcome.facts.outcome.as_deref(),
        Some("already_loaded")
    );
    assert_eq!(
        again.outcome.content, "outcome: already_loaded\nskill: alpha\n",
        "a repeat is the tool's own quiet result: {}",
        again.outcome.content
    );
    assert_eq!(h.loaded(), vec!["alpha".to_string()]);
    h.run("follow the steps");
    let first = &h.conversations()[0];
    assert_eq!(
        first.len(),
        3,
        "the repeat is not an exchange the model reads: {first:?}"
    );
}

#[test]
fn a_user_only_skill_loads_by_hand_and_is_never_in_the_models_tool() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    skills::write(
        workspace.path(),
        "house",
        "disable-model-invocation: true\n",
        "Never rewrite a file.",
    );
    let alpha_only = alpha(workspace.path());
    assert!(
        alpha_only.skills.iter().any(|skill| skill.name == "house"),
        "the scan found it"
    );
    let mut h = Harness::skilled(vec![answer("Done.")], Vec::new(), &alpha_only);

    let activated = h.activate("house");

    assert!(activated.loaded, "a human may reach it: {activated:?}");
    assert!(
        activated.outcome.content.contains("Never rewrite a file."),
        "{}",
        activated.outcome.content
    );
    let built = SkillTool::new(&alpha_only).expect("alpha is a skill the model may see");
    assert!(
        !built.spec().description.contains("house"),
        "and the catalog the model is handed never carried it"
    );
}

#[test]
fn a_model_only_skill_cannot_be_loaded_by_hand() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    skills::write(
        workspace.path(),
        "hidden",
        "user-invocable: false\n",
        "Follow this silently.",
    );
    let catalog = alpha(workspace.path());
    let mut h = Harness::skilled(vec![answer("Done.")], Vec::new(), &catalog);

    let activated = h.activate("hidden");

    assert!(!activated.loaded);
    assert!(activated.outcome.is_error, "{activated:?}");
    assert!(
        activated
            .outcome
            .content
            .contains("there is no skill called `hidden`"),
        "{}",
        activated.outcome.content
    );
    assert_eq!(
        activated.outcome.facts.reason.as_deref(),
        Some("there is no skill called `hidden`: `skill` takes one of `alpha`"),
        "the reason a frame says is the fact the result carries, in its own words"
    );
    assert!(h.loaded().is_empty(), "nothing was put in force");
}

#[test]
fn a_load_by_hand_is_forgotten_with_the_history_it_was_told_in() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let mut h = Harness::skilled(vec![answer("Done.")], Vec::new(), &catalog);

    h.activate("alpha");
    h.forget();
    let again = h.activate("alpha");

    assert!(
        again.loaded,
        "a history that was told nothing holds nothing: {again:?}"
    );
}
