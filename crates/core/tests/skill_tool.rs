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

use sindri_core::agent::{BashTool, DOCTRINE, Limits, SkillTool, Tools};
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
    // the catalog the fold built, and in nothing the model is handed.
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
    // What a model cannot read off the schema or learn from calling: the key a
    // task is matched on, the brake on over-triggering and the
    // once-per-conversation answer.
    for promised in ["a line below", "and not otherwise", "already_loaded"] {
        assert!(
            spec.description.contains(promised),
            "the description says `{promised}`: {}",
            spec.description
        );
    }
    assert!(
        spec.parameters["properties"]["name"]
            .get("description")
            .is_none(),
        "the enum is the contract, and the catalog is beside this field: {}",
        spec.parameters["properties"]["name"]
    );
    assert_eq!(spec.parameters["type"], "object");
    assert_eq!(spec.parameters["required"][0], "name");
    assert_eq!(
        spec.parameters["properties"]["name"]["enum"],
        serde_json::json!(["alpha"]),
        "the name parameter is the set of names that may be called"
    );
}

#[test]
fn the_description_opens_with_the_doctrine_compiled_in_beside_this_module() {
    // The doctrine is the prose, not the catalog: it is a text of its own so a
    // bench can move the wording of the one sentence that asks for a load
    // without touching a line of product code, and the description is built
    // from that file and nothing else — a second copy would be a second answer
    // to what the model was told.
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let tool = tool(&catalog);

    let description = &tool.spec().description;
    assert!(
        description.starts_with(DOCTRINE),
        "the description opens with the compiled doctrine: {description}"
    );
    assert!(
        description
            .trim_end()
            .ends_with("- alpha: what alpha is for"),
        "and ends with the catalog, which changes with the workspace: {description}"
    );
    assert!(
        DOCTRINE.contains(TRIGGER),
        "the trigger sentence is one line of the doctrine, which is what a bench moves it by"
    );
    assert!(
        DOCTRINE.ends_with('\n'),
        "the doctrine ends a line, so the catalog's heading opens its own: {DOCTRINE:?}"
    );
    assert!(
        !DOCTRINE.contains("Skills:"),
        "the catalog's heading belongs to the catalog: {DOCTRINE}"
    );
}

/// The sentence a run is asked to load a skill by, and the one the bench's third
/// arm removes: one line, so the arm differs by that line alone, brake included.
const TRIGGER: &str = "Call it before acting when a task is one it covers, and not otherwise.";

#[test]
fn the_description_says_nothing_a_result_or_another_tool_already_says() {
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

    // `bash` is in every run, and it states the boundary: the paths, the
    // timeout, the wall clock. Nothing of that belongs here, where it would be
    // re-sent on every turn of every run that holds a skill to say the same thing
    // twice.
    assert!(
        bash.spec().description.contains("Writable paths:"),
        "the boundary is the tool every run has, and it states it"
    );
    let description = &skill.description;
    for duplicated in ["Writable paths:", "time-bounded"] {
        assert!(
            !description.contains(duplicated),
            "so `skill` spends no word on `{duplicated}`: {description}"
        );
    }
    // The words that mean the shape of a load result has crept back in: a model
    // learns that shape by calling, once, and every word of it is re-sent on
    // every turn of every run that holds a skill. Asked of the doctrine, which
    // is the tool's own prose — a catalog line is a skill's description, and
    // whatever words a workspace puts there are that skill's to spend.
    for crept in ["frontmatter", "files", "directory", "budget"] {
        assert!(
            !DOCTRINE.contains(crept),
            "the doctrine says nothing of `{crept}`, which the result says: {DOCTRINE}"
        );
    }
    assert!(
        description.contains("already_loaded"),
        "what the schema cannot carry is still said: {description}"
    );
    assert!(
        description.contains("- alpha: what alpha is for"),
        "and the catalog is where the schema's names are matched: {description}"
    );
}

#[test]
fn the_doctrine_is_two_sentences_and_then_the_catalog_in_its_own_order() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let mut catalog = alpha(workspace.path());
    let mut second = catalog.skills[0].clone();
    second.name = "beta".to_string();
    second.description = "what beta is for".to_string();
    catalog.skills.push(second);
    let built = tool(&catalog);
    let description = &built.spec().description;

    // Two sentences and no more: what the tool loads and the line to match a
    // task on, in one; when to call it, in the other. A sentence is what ends a
    // line here, so the third one would be the line that bought nothing.
    assert_eq!(
        DOCTRINE.matches(".\n").count(),
        2,
        "the doctrine is two sentences, each ending its line: {DOCTRINE}"
    );
    assert!(
        DOCTRINE.len() <= DOCTRINE_CHARS,
        "and spends no more than the {DOCTRINE_CHARS} characters it was cut to, not {}: \
         {DOCTRINE}",
        DOCTRINE.len()
    );

    // The catalog is the doctrine's second half and nothing else: one heading,
    // then the skills in the order the scan found them, which is the order the
    // schema's enum offers them in.
    let listed: String = catalog
        .skills
        .iter()
        .map(|skill| format!("- {}: {}\n", skill.name, skill.description))
        .collect();
    assert_eq!(
        description.strip_prefix(DOCTRINE),
        Some(format!("Skills:\n{listed}").as_str()),
        "the catalog follows the doctrine in its own order: {description}"
    );
    assert_eq!(
        built.spec().parameters["properties"]["name"]["enum"],
        serde_json::json!(["alpha", "beta"]),
        "and the enum is that catalog's order, unchanged"
    );
}

/// The most characters the doctrine may spend, the newline that ends its last
/// sentence included: the figure it was cut to when the sentence narrating a
/// result's shape left, kept here so the prose cannot creep back one clause at a
/// time on a text every turn of every run that holds a skill pays for.
const DOCTRINE_CHARS: usize = 227;

#[test]
fn the_description_sends_no_word_on_where_a_body_path_resolves() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let built = tool(&catalog);
    let description = &built.spec().description;

    // A scanned skill's directory is in the result, and so is the sentence that
    // says a path written in the body resolves from it: the description is
    // re-sent on every turn of every run that holds a skill, and a fact about
    // one activation is paid once, in the result of that activation.
    assert!(
        !description.contains("relative reference"),
        "the description leaves the resolution clause to the result: {description}"
    );

    // And the result is where it is paid: the load says it beside the path.
    let outcome = built.call(&load("call_1", "alpha"), "alpha", &Default::default());
    assert!(
        outcome
            .content
            .contains("a path written in the body resolves from here"),
        "the result carries the clause the description dropped: {}",
        outcome.content
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
    let dir = catalog.skills[0]
        .dir()
        .expect("a scanned skill has a directory")
        .display()
        .to_string();

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
    // A load that does carry files still lists them, under the heading and in
    // the scan's own order, one line each: the heading is absent where there is
    // nothing to name and never a list with nothing under it.
    assert!(
        outcome
            .content
            .contains("files:\n  SKILL.md\n  references/\n  references/api.md\n"),
        "the listing opens with its heading where there is a file to name: {}",
        outcome.content
    );
    // The sentence the tool's description used to carry: a path written in the
    // body resolves from the directory beside it. It belongs here, where it is
    // paid once per activation, on the one line that names that directory.
    let clause = format!("dir: {dir} — a path written in the body resolves from here");
    assert!(
        outcome.content.lines().any(|line| line == clause),
        "the directory is answered with the resolution clause on it, once: {}",
        outcome.content
    );
    assert_eq!(
        outcome.content.matches("resolves from here").count(),
        1,
        "one line, one place: {}",
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
fn a_listing_past_the_most_one_load_shows_ends_with_the_count_it_left_out() {
    let workspace = tempfile::TempDir::new().expect("workspace");
    let catalog = alpha(workspace.path());
    let dir = catalog.skills[0]
        .dir()
        .expect("a scanned skill has a directory");
    // Past the 200 one activation lists, so the listing has more to name than
    // the result spends.
    for index in 0..203 {
        skills::reference(dir, &format!("references/note-{index:03}.md"), "# a note\n");
    }
    let listing = catalog.skills[0].listing();
    assert_eq!(
        listing.len(),
        203 + 3,
        "the walk found them all: {listing:?}"
    );
    let tool = tool(&catalog);

    let outcome = tool.call(&load("call_1", "alpha"), "alpha", &Default::default());

    let shown: Vec<&str> = outcome
        .content
        .lines()
        .skip_while(|line| *line != "files:")
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .collect();
    let counted = format!("  ({} more)", listing.len() - 200);
    assert_eq!(
        shown.len(),
        201,
        "the 200 files one load lists and the line that counts the rest: {shown:?}"
    );
    assert_eq!(
        shown.last(),
        Some(&counted.as_str()),
        "a long listing ends with what it left out: {shown:?}"
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
    let dir = catalog.skills[0]
        .dir()
        .expect("a scanned skill has a directory");
    std::fs::remove_file(dir.join("SKILL.md")).expect("the file goes");

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
