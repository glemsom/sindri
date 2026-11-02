//! The `skill` tool's contract with the model: the catalog and the doctrine in
//! the description it was built with, a name the schema lists, one body per
//! conversation, and a result that says where the skill lives. What a run's
//! record and a front end's row make of the same call is proven beside the two
//! that read them.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

mod support;

use std::path::Path;

use sindri_core::agent::{BashTool, Limits, SkillTool, Tools};
use sindri_core::provider::{Asked, Call, Turn};
use sindri_core::skills::Catalog;
use support::{Harness, Scripted, answer, calls, fixtures, skills};

/// A call the model makes: `skill` by name, or by whatever it said instead.
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

/// A turn that asks for these calls and says nothing else.
fn turn(asked: Vec<Call>) -> Turn {
    calls(asked)
}

fn tool(catalog: &Catalog) -> SkillTool {
    SkillTool::new(catalog).expect("the catalog holds a skill the model may see")
}

/// A workspace with one model-visible skill, `alpha`, holding a body and a
/// reference beside it.
fn alpha(workspace: &Path) -> Catalog {
    let dir = skills::write(
        workspace,
        "alpha",
        "",
        "Follow the steps.\nSee `references/api.md` for the shape of a note.",
    );
    skills::reference(&dir, "references/api.md", "# the api\n");
    skills::catalog(workspace)
}

#[test]
fn with_no_skill_the_model_may_see_the_tool_is_not_registered() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    // A house rule a human may reach for and the model may not see: it is in
    // the catalog the scan found, and in nothing the model is handed.
    skills::write(
        workspace.path(),
        "house",
        "disable-model-invocation: true\n",
        "Never rewrite a file.",
    );
    let catalog = skills::catalog(workspace.path());

    assert_eq!(catalog.skills.len(), 1, "the scan still found it");
    assert!(
        SkillTool::new(&catalog).is_none(),
        "no model-visible skill means no tool at all"
    );
    let (_workspace, _session_temp, policy) = fixtures();
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    assert_eq!(
        Tools::new(bash, None, None).specs().len(),
        1,
        "the model is handed bash and nothing else"
    );
}

#[test]
fn the_catalog_and_the_doctrine_are_in_the_tool_it_describes() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let tool = tool(&catalog);

    let spec = tool.spec();
    assert_eq!(spec.name, "skill");
    // Not the cost of a result: `bash` states it in the same request, and a
    // sentence paid on every turn to repeat it buys nothing.
    for promised in [
        "already_loaded",
        "frontmatter stripped",
        "- alpha: what alpha is for",
    ] {
        assert!(
            spec.description.contains(promised),
            "the description says `{promised}`: {}",
            spec.description
        );
    }
    assert_eq!(spec.parameters["type"], "object");
    assert_eq!(spec.parameters["required"][0], "name");
    assert_eq!(
        spec.parameters["properties"]["name"]["enum"],
        serde_json::json!(["alpha"]),
        "the name parameter is the set of names that may be called"
    );
}

#[test]
fn the_skill_description_does_not_restate_the_rule_the_bash_description_states() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    // `bash` is in every run, and it already says what a result costs: it is
    // carried and re-sent on every later turn. The one sentence that repeats it
    // here is paid on every turn of every run that has a skill, to buy what the
    // request has already bought one tool over.
    let (_workspace, _session_temp, policy) = fixtures();
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    let built = tool(&catalog);
    let skill = built.spec();

    assert!(
        bash.spec()
            .description
            .contains("sent again with every later turn"),
        "the cost of a result is stated by the tool every run has"
    );
    let description = &skill.description;
    assert!(
        !description.contains("sent again with every later turn"),
        "so `skill` spends no word on it: {description}"
    );
    assert!(
        !description.contains("budget"),
        "nor on the cost of a body, which is a result like any other: {description}"
    );
    assert!(
        description.contains("already_loaded"),
        "what the schema cannot carry is still said: {description}"
    );
}

#[test]
fn a_user_only_skill_is_in_neither_the_schema_nor_the_catalog() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let mut catalog = alpha(workspace.path());
    let mut house = catalog.skills[0].clone();
    house.name = "house".to_string();
    house.description = "a rule the model may not see".to_string();
    house.model_invocable = false;
    catalog.skills.push(house);

    let built = tool(&catalog);
    assert!(
        !built.spec().description.contains("house"),
        "a user-only skill is not in the catalog the tool carries: {}",
        built.spec().description
    );
    assert_eq!(
        built.spec().parameters["properties"]["name"]["enum"],
        serde_json::json!(["alpha"])
    );
    let outcome = built.call(&load("call_1", "house"), "house", &Default::default());
    assert!(outcome.is_error, "and it cannot be called either");
    assert!(outcome.content.contains("no skill called `house`"));
}

#[test]
fn an_unknown_name_is_refused_with_the_names_that_work() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let tool = tool(&catalog);

    let outcome = tool.call(&load("call_1", "alpah"), "alpah", &Default::default());

    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("`alpah`") && outcome.content.contains("`alpha`"),
        "the model is told what it asked for and what it could have asked for: {}",
        outcome.content
    );
}

#[test]
fn activation_returns_the_body_inside_a_named_wrapper_with_its_directory_and_files() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let tool = tool(&catalog);
    let dir = catalog.skills[0].dir.display().to_string();

    let outcome = tool.call(&load("call_1", "alpha"), "alpha", &Default::default());

    assert!(!outcome.is_error, "{}", outcome.content);
    assert!(outcome.content.contains("outcome: loaded"));
    assert!(
        outcome
            .content
            .contains("<skill_content name=\"alpha\">\nFollow the steps."),
        "the body is in a named wrapper: {}",
        outcome.content
    );
    assert!(
        !outcome.content.contains("description:") && !outcome.content.contains("---\n"),
        "the frontmatter is stripped: {}",
        outcome.content
    );
    assert!(
        outcome.content.contains(&dir) && outcome.content.contains("  references/api.md"),
        "the directory and the files beside the body come with it: {}",
        outcome.content
    );
    assert!(
        outcome
            .content
            .contains("references/api.md` for the shape of a note.\n</skill_content>"),
        "the body ends where the wrapper does: {}",
        outcome.content
    );
}

#[test]
fn a_second_call_is_already_loaded_and_carries_no_body() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let tool = tool(&catalog);
    let loaded = sindri_core::agent::Loaded::default();

    let first = tool.call(&load("call_1", "alpha"), "alpha", &loaded);
    let second = tool.call(&load("call_2", "alpha"), "alpha", &loaded);

    assert!(first.content.contains("<skill_content"));
    assert_eq!(
        second.content, "outcome: already_loaded\nskill: alpha\n",
        "a body the conversation already holds is not paid for twice"
    );
    assert!(!second.is_error, "it is an answer, not a failure");
    assert_eq!(
        second.facts.outcome.as_deref(),
        Some("already_loaded"),
        "the fields say what the content opens with"
    );
}

#[test]
fn a_skill_file_that_cannot_be_read_is_a_result_that_names_it() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let tool = tool(&catalog);
    std::fs::remove_file(catalog.skills[0].dir.join("SKILL.md")).expect("the file goes");

    let outcome = tool.call(&load("call_1", "alpha"), "alpha", &Default::default());

    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("outcome: unreadable") && outcome.content.contains("SKILL.md"),
        "the model is told which file and why: {}",
        outcome.content
    );
}

#[test]
fn a_skill_call_inside_one_wave_is_one_load() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    // Two identical calls in one turn: the wave is one turn, and one body is
    // what it leaves behind.
    let mut h = Harness::skilled(
        vec![
            turn(vec![load("call_1", "alpha"), load("call_2", "alpha")]),
            answer("Followed it."),
        ],
        Vec::new(),
        &catalog,
    );

    let report = h.run("follow the team's steps");

    assert_eq!(report.tool_calls, 2, "both calls were answered");
    assert_eq!(h.loaded(), vec!["alpha".to_string()]);
    let conversations = h.conversations();
    let results = match &conversations[1][2] {
        sindri_core::provider::Message::ToolResults(results) => results,
        other => panic!("the turn's results are fed back: {other:?}"),
    };
    // Which of the two threads wins the load is a race; that exactly one does
    // is the promise, and the wave is what keeps it.
    let bodies = results
        .iter()
        .filter(|result| result.content.contains("<skill_content"))
        .count();
    let quiet = results
        .iter()
        .filter(|result| result.content == "outcome: already_loaded\nskill: alpha\n")
        .count();
    assert_eq!((bodies, quiet), (1, 1), "{results:?}");
}

#[test]
fn the_loaded_set_is_cleared_when_the_history_is() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let mut h = Harness::skilled(
        vec![
            turn(vec![load("call_1", "alpha")]),
            answer("Read it."),
            turn(vec![load("call_2", "alpha")]),
            answer("Read it again."),
        ],
        Vec::new(),
        &catalog,
    );

    h.run("first");
    assert_eq!(h.loaded(), vec!["alpha".to_string()]);
    h.forget();
    assert!(
        h.loaded().is_empty(),
        "a history that was told nothing holds nothing"
    );
    h.run("second");

    let conversations = h.conversations();
    let results = match &conversations[1][2] {
        sindri_core::provider::Message::ToolResults(results) => results,
        other => panic!("the turn's results are fed back: {other:?}"),
    };
    assert!(
        results[0].content.contains("<skill_content"),
        "a new history is told the body again: {}",
        results[0].content
    );
}

#[test]
fn a_skill_body_is_charged_to_the_budget_as_any_result_is() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let tool = tool(&catalog);
    let outcome = tool.call(&load("call_1", "alpha"), "alpha", &Default::default());

    // The body is a tool result like any other: what the conversation keeps is
    // what the run pays for, so a body that fills the budget stops the run
    // exactly as a command's output would. There is no exemption to be had.
    let mut h = Harness::skilled(
        vec![turn(vec![load("call_1", "alpha")]), answer("Done.")],
        Vec::new(),
        &catalog,
    );
    h.set_budget(sindri_core::agent::Budget {
        wall: std::time::Duration::from_secs(3600),
        bytes: outcome.content.len() as u64,
    });
    let report = h.run("read the skill");

    assert_eq!(
        report.stopped,
        sindri_core::agent::Stopped::Budget,
        "a body is charged to the budget as any other result is"
    );
    assert_eq!(report.spent.bytes, outcome.content.len() as u64);
}

#[test]
fn a_skill_call_in_a_run_without_the_tool_is_answered_not_run() {
    let (_workspace, _session_temp, policy) = fixtures();
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    let tools = Tools::new(bash, None, None);

    let outcome = tools
        .call(&load("call_1", "alpha"), &Default::default())
        .expect("a call is answered even when no tool can run it");

    assert!(outcome.is_error);
    assert!(
        outcome.content.contains("no `skill` tool in this run"),
        "{}",
        outcome.content
    );
    assert_eq!(
        load("call_1", "alpha").asked(),
        Asked::Skill {
            name: "alpha".to_string()
        }
    );
}
