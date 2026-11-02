//! A run against a repository whose skill the task needs: the model is handed
//! the catalog in the tool it would call, loads one by name, answers in
//! accordance with it, and asks for it a second time for nothing. Everything
//! here is asserted from outside the binary — the request bodies and the record
//! the run leaves.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::provider::{MockProvider, Reply};
use common::{Env, batch, stderr, stdout, the_session};

/// A skill in the home scope, which is where a user's own collection lives.
fn skill(env: &Env, name: &str, body: &str) {
    let dir = env.home.join(".agents/skills").join(name);
    std::fs::create_dir_all(&dir).expect("skill directory");
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: the team's own voice\n---\n\n{body}\n"),
    )
    .expect("SKILL.md");
}

#[test]
fn a_run_loads_the_skill_it_needs_and_the_record_says_which() {
    let env = Env::new();
    skill(
        &env,
        "house-style",
        "Answer in short sentences. See `references/more.md`.",
    );
    let mock = MockProvider::start(vec![
        Reply::skill_call("t1", "house-style", ""),
        Reply::skill_call("t2", "house-style", ""),
        Reply::answer("Short. Done."),
    ]);

    let output = batch(&env, &mock, &["-b", "say something"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Short. Done.\n");

    let requests = mock.requests();
    assert_eq!(requests.len(), 3, "load, load again, answer");
    let tools = requests[0].json()["tools"].clone();
    let tools = tools.as_array().expect("the request carries tools");
    assert_eq!(tools.len(), 2, "bash and skill: {tools:?}");
    let skill = tools
        .iter()
        .find(|tool| tool["name"] == "skill")
        .expect("the skill tool is registered");
    assert!(
        skill["description"]
            .as_str()
            .expect("a description")
            .contains("house-style: the team's own voice"),
        "the catalog is in the description: {}",
        skill["description"]
    );
    // Every catalog the binary builds holds the skills it ships, beside the
    // tree's own: both are names the model may call.
    let mut names = vec!["house-style".to_string()];
    names.extend(
        sindri_core::skills::builtins()
            .skills
            .iter()
            .map(|shipped| shipped.name.clone()),
    );
    names.sort();
    assert_eq!(
        skill["input_schema"]["properties"]["name"]["enum"],
        serde_json::json!(names),
        "and the name parameter is the set of names it may call"
    );

    let first = &requests[1].body;
    assert!(
        first.contains("<skill_content name=\\\"house-style\\\">")
            && first.contains("Answer in short sentences."),
        "the body comes back inside its wrapper: {first}"
    );
    assert!(
        requests[2].body.contains("already_loaded"),
        "the second call is answered without a second body: {}",
        requests[2].body
    );

    let transcript = std::fs::read_to_string(the_session(&env).join("transcript.jsonl"))
        .expect("transcript.jsonl");
    let calls: Vec<serde_json::Value> = transcript
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|entry| entry["type"] == "tool_call")
        .collect();
    assert_eq!(calls.len(), 2, "both calls are on the record: {calls:?}");
    assert_eq!(calls[0]["tool"], "skill", "the record says which tool");
    assert_eq!(calls[0]["name"], "house-style", "and which skill");
}

#[test]
fn a_skill_the_model_may_not_see_is_not_a_name_it_is_handed() {
    let env = Env::new();
    skill(&env, "house-style", "Answer in short sentences.");
    // A skill the model may not see is still a skill a human may run: it is in
    // the catalog `doctor` prints and in nothing the model is handed.
    let dir = env.home.join(".agents/skills/house");
    std::fs::create_dir_all(&dir).expect("skill directory");
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: house\ndescription: a rule for humans\ndisable-model-invocation: true\n---\n\nNever.\n",
    )
    .expect("SKILL.md");
    std::fs::remove_dir_all(env.home.join(".agents/skills/house-style"))
        .expect("the visible skill goes");

    let mock = MockProvider::start(vec![Reply::answer("Nothing to load.")]);
    let output = batch(&env, &mock, &["-b", "hello"], "");

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let tools = mock.requests()[0].json()["tools"].clone();
    let tools = tools.as_array().expect("the request carries tools").clone();
    let skill = tools
        .iter()
        .find(|tool| tool["name"] == "skill")
        .expect("the built-in keeps the tool registered");
    let names = skill["input_schema"]["properties"]["name"]["enum"]
        .as_array()
        .expect("an enum of names")
        .clone();
    assert!(
        !names.iter().any(|name| name == "house"),
        "a user-only skill is in no schema: {names:?}"
    );
    let shipped: Vec<serde_json::Value> = sindri_core::skills::builtins()
        .skills
        .iter()
        .map(|known| serde_json::json!(known.name))
        .collect();
    assert_eq!(
        names, shipped,
        "what is left is the catalog the binary ships: {names:?}"
    );
}
