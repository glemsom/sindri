//! A built-in through the `skill` tool: a body with no directory behind it, the
//! files that come with it instead of a path the model would open, the two
//! invocation flags, and the once-per-conversation claim — every one of them
//! read by the same code a scanned skill goes through.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use sindri_core::agent::{SkillTool, UserSkills};
use sindri_core::provider::{Call, Message};
use sindri_core::skills::{Catalog, Skill, builtins, fold};
use support::{Harness, answer, calls};

/// The built-in this file is written about: the ticket ships one.
fn one() -> Skill {
    builtins()
        .skills
        .into_iter()
        .next()
        .expect("the binary ships a skill")
}

/// A call the model makes: `skill` by name.
fn load(id: &str, name: &str) -> Call {
    Call::new(
        id,
        "skill",
        Ok(serde_json::Map::from_iter([(
            "name".to_string(),
            serde_json::json!(name),
        )])),
    )
}

fn tool(catalog: &Catalog) -> SkillTool {
    SkillTool::new(catalog).expect("the built-in is one the model may see")
}

#[test]
fn a_builtin_activates_to_its_body_and_its_files_with_no_path_to_open() {
    let catalog: Catalog = fold([builtins()]);
    let found = one();
    let built = tool(&catalog);

    let outcome = built.call(
        &load("call_1", &found.name),
        &found.name,
        &Default::default(),
    );

    assert!(!outcome.is_error, "{}", outcome.content);
    assert!(outcome.content.starts_with("outcome: loaded\n"));
    assert!(
        outcome
            .content
            .contains(&format!("<skill_content name=\"{}\">", found.name))
    );
    // The field a scanned activation opens with, and not the string: a body is
    // prose and may quote a shell's own `mkdir:` while saying what a refusal
    // looks like, which is a line no reader could mistake for this wrapper's.
    assert!(
        !outcome.content.lines().any(|line| line.starts_with("dir:")),
        "there is no directory, and a path that does not exist is not invented: {}",
        outcome.content
    );
    // The files come with the body because there is no directory for the model
    // to open one out of, so a relative reference in the body reads where it
    // points rather than leading nowhere.
    for file in found.files() {
        assert!(
            outcome
                .content
                .contains(&format!("<skill_file path=\"{}\">", file.path)),
            "{} comes with the body: {}",
            file.path,
            outcome.content
        );
        assert!(
            outcome.content.contains(file.text.trim()),
            "and its own words come with it: {}",
            outcome.content
        );
    }
}

#[test]
fn two_calls_for_a_builtin_in_one_wave_leave_one_body() {
    let catalog: Catalog = fold([builtins()]);
    let name = one().name;
    let mut h = Harness::skilled(
        vec![
            calls(vec![load("call_1", &name), load("call_2", &name)]),
            answer("Followed the built-in."),
        ],
        Vec::new(),
        &catalog,
    );

    let report = h.run("do the work");

    assert_eq!(report.tool_calls, 2, "both calls were answered");
    assert_eq!(h.loaded(), vec![name.clone()], "one name, claimed once");
    let conversations = h.conversations();
    let results = match &conversations[1][2] {
        Message::ToolResults(results) => results,
        other => panic!("the turn's results are fed back: {other:?}"),
    };
    let bodies = results
        .iter()
        .filter(|result| result.content.contains("<skill_content"))
        .count();
    let quiet = results
        .iter()
        .filter(|result| result.content == format!("outcome: already_loaded\nskill: {name}\n"))
        .count();
    assert_eq!((bodies, quiet), (1, 1), "{results:?}");
}

#[test]
fn a_builtin_carries_no_body_into_a_conversation_that_already_has_it() {
    let catalog: Catalog = fold([builtins()]);
    let name = one().name;
    let built = tool(&catalog);
    let loaded = sindri_core::agent::Loaded::default();

    let first = built.call(&load("call_1", &name), &name, &loaded);
    let second = built.call(&load("call_2", &name), &name, &loaded);

    assert!(first.content.contains("<skill_content"));
    assert_eq!(
        second.content,
        format!("outcome: already_loaded\nskill: {name}\n"),
        "a built-in's body is paid for once, like any other"
    );
    assert_eq!(second.facts.outcome.as_deref(), Some("already_loaded"));
}

#[test]
fn a_builtin_is_held_to_the_two_invocation_flags_as_a_scanned_skill_is() {
    let mut catalog: Catalog = fold([builtins()]);
    let name = one().name;

    // `disable-model-invocation: true`: in no catalog the model reads and in no
    // schema it chooses from, and still a skill a human may run.
    catalog.skills[0].model_invocable = false;
    assert!(
        SkillTool::new(&catalog).is_none(),
        "a user-only built-in registers no tool at all"
    );
    assert_eq!(
        UserSkills::new(&catalog).names(),
        vec![name.clone()],
        "the composer may still reach it"
    );

    // `user-invocable: false`: the other way round.
    catalog.skills[0].model_invocable = true;
    catalog.skills[0].user_invocable = false;
    assert!(
        UserSkills::new(&catalog).names().is_empty(),
        "a model-only built-in is in no completion"
    );
    let built = tool(&catalog);
    assert_eq!(
        built.spec().parameters["properties"]["name"]["enum"],
        serde_json::json!([name]),
        "and is the one name the schema offers"
    );
}
