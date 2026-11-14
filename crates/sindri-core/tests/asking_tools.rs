//! What the seam tells the wire about the tools a run holds, and what it does with a reply
//! that asks for one: a prompt asking for the files in a folder is answered by running `bash`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sindri_core::asking::Asking;
use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::provider::transport::{Canned, Replay};
use sindri_core::provider::{Ask, Client, Route};
use sindri_core::redact::{Redacted, SecretSet};
use sindri_core::run::{Acted, Call, Executor, Exit, Loaded, Run, State, Stop};
use sindri_core::skills::Catalog;
use sindri_core::tools::{FrontEnd, ToolSet};
use sindri_core::wire::Wire;

const PLAIN: &str = include_str!("fixtures/providers/anthropic_stream.sse");

const TOOL_USE: &str = include_str!("fixtures/providers/anthropic_tool_use.sse");

fn client_over(replay: &Replay) -> Client {
    Client::over(replay.clone(), SecretSet::default()).waits(|_| {})
}

fn ask() -> Ask {
    Ask::new(Wire::Anthropic, "claude-sonnet-4-6")
        .key("sk-ant-recorded")
        .preamble("## Persona\n\nYou are Sindri.")
}

fn tools() -> ToolSet {
    let workspace = tempfile::tempdir().expect("a temp dir");
    let boundary = Boundary::from_config(
        &Config::default(),
        workspace.path(),
        Enforcement::Bubblewrap,
    )
    .expect("the boundary holds");
    ToolSet::new(&boundary, &Catalog::default(), FrontEnd::Batch)
}

#[derive(Clone, Default)]
struct Ran(Arc<Mutex<Vec<Call>>>);

impl Ran {
    fn names(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|call| call.name.clone())
            .collect()
    }
}

impl Executor for Ran {
    fn execute(&self, call: &Call, _loaded: &mut Loaded) -> Result<Acted, Redacted> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(call.clone());
        Ok(Acted {
            text: String::from("Cargo.toml\nGLOSSARY.md\n"),
            bytes: 26,
            elapsed: Duration::ZERO,
            exit: Exit::Code(0),
        })
    }
}

#[test]
fn the_wire_is_asked_with_the_bash_tool_the_run_holds() {
    let replay = Replay::new([Canned::stream(PLAIN)]);
    let tools = tools();
    let mut run = Run::new(
        "List files in this folder",
        Asking::new(client_over(&replay), ask(), tools.clone()),
        Ran::default(),
    )
    .tools(&tools);

    run.drive(&mut ());

    let body = &replay.requests()[0].body;
    assert!(
        body.contains("\"name\":\"bash\""),
        "the wire is told the run's `bash` tool exists: {body}"
    );
    assert!(
        body.contains("\"command\""),
        "and the one argument it takes: {body}"
    );
}

#[test]
fn a_reply_that_asks_for_bash_runs_it_and_answers() {
    let replay = Replay::new([Canned::stream(TOOL_USE), Canned::stream(PLAIN)]);
    let ran = Ran::default();
    let tools = tools();
    let mut run = Run::new(
        "List files in this folder",
        Asking::new(client_over(&replay), ask(), tools.clone()),
        ran.clone(),
    )
    .tools(&tools);

    let state = run.drive(&mut ());

    assert_eq!(state, State::Stopped(Stop::Final), "the run answers");
    assert_eq!(ran.names(), ["bash"], "the call the reply asked for ran");
    assert_eq!(
        run.history()[0].calls.len(),
        1,
        "and the turn it ran in says so"
    );
}

#[test]
fn the_output_of_a_call_is_what_the_next_ask_is_told() {
    let replay = Replay::new([Canned::stream(TOOL_USE), Canned::stream(PLAIN)]);
    let tools = tools();
    let mut run = Run::new(
        "List files in this folder",
        Asking::new(client_over(&replay), ask(), tools.clone()),
        Ran::default(),
    )
    .tools(&tools);

    run.drive(&mut ());

    let requests = replay.requests();
    assert_eq!(
        requests.len(),
        2,
        "the run asks again with what the call answered"
    );
    let second = &requests[1].body;
    assert!(
        second.contains("\"tool_use\""),
        "the call the model made is replayed as its own turn: {second}"
    );
    assert!(
        second.contains("\"tool_use_id\":\"toolu_01\""),
        "answered against the wire's own identity for it: {second}"
    );
    assert!(
        second.contains("Cargo.toml"),
        "and what the command printed is the result it reads: {second}"
    );
    assert_eq!(
        run.answer(),
        Some("Hello world"),
        "the run answers with the words of the turn that followed"
    );
}

const OPENAI_TOOL_USE: &str = include_str!("fixtures/providers/openai_chat_tool_use.sse");

const OPENAI_PLAIN: &str = include_str!("fixtures/providers/copilot_chat_stream.sse");

#[test]
fn an_openai_shaped_wire_is_told_the_tools_and_answers_with_a_call_too() {
    let replay = Replay::new([
        Canned::stream(OPENAI_TOOL_USE),
        Canned::stream(OPENAI_PLAIN),
    ]);
    let ran = Ran::default();
    let tools = tools();
    let mut run = Run::new(
        "List files in this folder",
        Asking::new(
            client_over(&replay),
            Ask::new(Wire::Openai, "gpt-4o-2024-11-20")
                .key("sk-recorded")
                .route(Route::Chat),
            tools.clone(),
        ),
        ran.clone(),
    )
    .tools(&tools);

    let state = run.drive(&mut ());

    assert_eq!(state, State::Stopped(Stop::Final));
    assert!(
        replay.requests()[0].body.contains("\"name\":\"bash\""),
        "the second dialect is told the same tool: {}",
        replay.requests()[0].body
    );
    assert_eq!(ran.names(), ["bash"], "and its call reaches the executor");
    assert_eq!(run.answer(), Some("Hello from Copilot"));
}
