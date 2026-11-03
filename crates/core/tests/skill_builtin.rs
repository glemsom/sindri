//! A built-in through the `skill` tool: a body with no directory behind it, the
//! files that come with it instead of a path the model would open, the two
//! invocation flags, and the once-per-conversation claim — every one of them
//! read by the same code a scanned skill goes through.
//!
//! Every test here loops over the whole table the binary ships rather than the
//! first skill in it: an invariant held only for the first is a skill added
//! later that nobody measured.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use sindri_core::agent::{Loaded, SkillTool, UserSkills};
use sindri_core::provider::{Call, Message};
use sindri_core::skills::{Catalog, Skill, builtins, fold};
use support::{Harness, answer, calls};

/// Every skill the binary ships, which is the set each invariant below is held
/// to: one skill proves a rule once, and the rule is what a fourth skill has to
/// meet before it is merged.
fn shipped() -> Vec<Skill> {
    let skills = builtins().skills;
    assert!(!skills.is_empty(), "the binary ships a skill");
    skills
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
    SkillTool::new(catalog).expect("the built-ins are skills the model may see")
}

#[test]
fn every_builtin_activates_to_its_body_and_the_files_it_ships_with_no_path_to_open() {
    let catalog: Catalog = fold([builtins()]);
    let built = tool(&catalog);

    for found in shipped() {
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
        // The field a scanned activation opens with, and not the string: a body
        // is prose and may quote a shell's own `mkdir:` while saying what a
        // refusal looks like, which is a line no reader could mistake for this
        // wrapper's.
        assert!(
            !outcome.content.lines().any(|line| line.starts_with("dir:")),
            "there is no directory, and a path that does not exist is not invented: {}",
            outcome.content
        );
        assert!(
            !outcome.content.contains("resolves from here"),
            "no clause is invented for a path a built-in does not have: {}",
            outcome.content
        );
        // The files come with the body because there is no directory for the
        // model to open one out of, so a relative reference in the body reads
        // where it points rather than leading nowhere.
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
}

#[test]
fn two_calls_for_a_builtin_in_one_wave_leave_one_body() {
    for found in shipped() {
        let catalog: Catalog = fold([builtins()]);
        let name = found.name;
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
}

#[test]
fn every_builtin_carries_no_body_into_a_conversation_that_already_has_it() {
    let catalog: Catalog = fold([builtins()]);
    let built = tool(&catalog);
    let loaded = Loaded::default();

    for found in shipped() {
        let first = built.call(&load("call_1", &found.name), &found.name, &loaded);
        let second = built.call(&load("call_2", &found.name), &found.name, &loaded);

        assert!(
            first.content.contains("<skill_content"),
            "{}",
            first.content
        );
        assert_eq!(
            second.content,
            format!("outcome: already_loaded\nskill: {}\n", found.name),
            "a built-in's body is paid for once, like any other"
        );
        assert_eq!(second.facts.outcome.as_deref(), Some("already_loaded"));
    }
}

#[test]
fn every_builtin_is_held_to_the_two_invocation_flags_as_a_scanned_skill_is() {
    let base: Catalog = fold([builtins()]);
    let names: Vec<String> = base.skills.iter().map(|skill| skill.name.clone()).collect();

    for (index, name) in names.iter().enumerate() {
        // `disable-model-invocation: true`: in no catalog the model reads and in
        // no schema it chooses from, and still a skill a human may run.
        let mut catalog = base.clone();
        catalog.skills[index].model_invocable = false;
        let offered = SkillTool::new(&catalog)
            .map(|built| {
                built.spec().parameters["properties"]["name"]["enum"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        assert!(
            !offered.contains(&serde_json::json!(name)),
            "a user-only built-in is in no schema: {offered:?}"
        );
        assert_eq!(
            UserSkills::new(&catalog).names(),
            names.clone(),
            "the composer may still reach it"
        );

        // `user-invocable: false`: the other way round.
        let mut catalog = base.clone();
        catalog.skills[index].user_invocable = false;
        assert!(
            !UserSkills::new(&catalog).names().contains(name),
            "a model-only built-in is in no completion"
        );
        let built = tool(&catalog);
        assert_eq!(
            built.spec().parameters["properties"]["name"]["enum"],
            serde_json::json!(names),
            "and is one of the names the schema offers"
        );
    }

    // A catalog nobody may see any of registers no tool at all.
    let mut none = base.clone();
    for skill in &mut none.skills {
        skill.model_invocable = false;
    }
    assert!(
        SkillTool::new(&none).is_none(),
        "a catalog of user-only built-ins registers no tool at all"
    );
}
