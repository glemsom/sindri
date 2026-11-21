//! What a launch is, proved at the core's own seam: one value that joins the boundary, the proof,
//! the session, the record, the tools and the client in the one order a run can be asked for.

use std::collections::{BTreeMap, VecDeque};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sindri_core::config::Config;
use sindri_core::launch::{Handed, Launch, Machine, Turns};
use sindri_core::open::{Handover, Opener, Opening};
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::provider::{Deltas, Failure, Usage};
use sindri_core::records::{Entry, SessionDocument, read_transcript};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Call, Cancel, Reply, Request, Responder, State, Stop, Turn};
use sindri_core::tools::{FrontEnd, OPEN, SKILL};
use sindri_core::wire::Wire;
use tempfile::TempDir;

/// A kernel that forbids unprivileged user namespaces: the stand-in answers a version and then
/// refuses the sandbox it was asked to make.
const RESTRICTED: &str = "if [ \"${1:-}\" = --version ]; then\n  \
                          echo 'bubblewrap 0.12.0 (stand-in)'\n  exit 0\nfi\n\
                          echo 'bwrap: setting up uid map: Permission denied' >&2\nexit 1";

/// A machine and the homes a launch reads, all under one temporary root, so nothing a launch
/// writes lands in the reader's own state.
struct Homes {
    root: TempDir,
    environment: BTreeMap<String, String>,
}

impl Homes {
    fn new() -> Homes {
        let root = tempfile::tempdir().expect("a temp home");
        let mut environment = BTreeMap::new();
        for (name, directory) in [
            ("XDG_STATE_HOME", "state"),
            ("HOME", "home"),
            ("XDG_CONFIG_HOME", "config"),
        ] {
            let path = root.path().join(directory);
            std::fs::create_dir_all(&path).expect("a directory");
            environment.insert(String::from(name), path.display().to_string());
        }
        Homes { root, environment }
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn machine(&self) -> Machine<'_> {
        Machine::new(&self.environment, self.at("bwrap"))
    }

    /// A stand-in bwrap on disk, so every refusal is provable where the real one works.
    fn machine_that_refuses(&self) -> Machine<'_> {
        let bwrap = self.at("bin").join("bwrap");
        std::fs::create_dir_all(self.at("bin")).expect("a machine directory");
        std::fs::write(&bwrap, format!("#!/bin/sh\n{RESTRICTED}\n")).expect("a stand-in bwrap");
        std::fs::set_permissions(&bwrap, Permissions::from_mode(0o755))
            .expect("the stand-in is executable");
        Machine::new(&self.environment, bwrap)
    }

    /// A skill under the homes' own home: the launch reads its workspace from the process, so a
    /// test cannot hand it a directory of its own.
    fn user_skill(&self, name: &str, description: &str) {
        let directory = self.at("home").join(".agents").join("skills").join(name);
        std::fs::create_dir_all(&directory).expect("a skill directory");
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\nBody of {name}.\n"),
        )
        .expect("a SKILL.md");
    }

    fn state_home(&self) -> &Path {
        self.environment
            .get("XDG_STATE_HOME")
            .map(Path::new)
            .expect("the homes name a state home")
    }
}

fn named(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(next) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => pending.push(path),
                _ if path.file_name().is_some_and(|found| found == name) => found.push(path),
                _ => {}
            }
        }
    }
    found
}

/// An endpoint nothing listens on: a scripted launch never asks it, and an opening that takes no
/// turn asks nothing.
const UNANSWERED_ENDPOINT: &str = "http://127.0.0.1:1";

fn naming_wire(base_url: String) -> Config {
    Config {
        provider: Some(Wire::Anthropic),
        model: Some(String::from("claude-sonnet-4-5")),
        api_key: Some(String::from("sk-ant-the-stand-in-one")),
        base_url: Some(base_url),
        ..Config::default()
    }
}

#[test]
fn a_config_that_names_no_run_is_refused_by_the_launch_in_one_vocabulary() {
    let homes = Homes::new();

    let Err(refusal) = Launch::open(
        &homes.machine(),
        &Config::default(),
        Enforcement::Bubblewrap,
        FrontEnd::Batch,
        None,
    ) else {
        panic!("a config naming neither a provider nor a model names no run");
    };

    assert_eq!(
        refusal.code(),
        2,
        "a config that holds no run is the usage refusal every front end already exits with"
    );
    assert!(
        refusal
            .message()
            .contains("a run needs a provider and a model"),
        "the refusal keeps the vocabulary the front ends were refused in: {}",
        refusal.message()
    );
}

#[test]
fn a_launch_refused_by_the_sandbox_keeps_no_record_and_asks_no_turn() {
    let homes = Homes::new();
    let machine = homes.machine_that_refuses();
    let script = Scripted::answering(Vec::new());

    let Err(refusal) = Launch::answered_by(
        &machine,
        &naming_wire(String::from(UNANSWERED_ENDPOINT)),
        Enforcement::Bubblewrap,
        FrontEnd::Batch,
        None,
        |_, _, _| script.clone(),
    ) else {
        panic!("this machine cannot make the sandbox");
    };

    assert_eq!(
        refusal.code(),
        3,
        "a launch with nothing to run with is the setup refusal: {}",
        refusal.message()
    );
    assert!(
        refusal
            .message()
            .contains("cannot create the sandbox: bubblewrap creates an unprivileged sandbox"),
        "the refusal is the sandbox card's own words: {}",
        refusal.message()
    );
    let documents = named(homes.state_home(), "session.json");
    assert!(
        documents.is_empty(),
        "nothing is recorded before the boundary is proved: {documents:?}"
    );
    assert!(
        script.asked().is_empty(),
        "a launch refused by the sandbox asks for no turn: {:?}",
        script.asked()
    );
}

/// A front end that writes no fragment: batch mode's answer is written once it is whole.
struct Quiet;

impl Deltas for Quiet {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

/// A responder scripted in advance: each request is logged and the next reply handed back, so a
/// launch's own loop is driven with no wire under it.
#[derive(Clone)]
struct Scripted {
    replies: Arc<Mutex<VecDeque<Reply>>>,
    asked: Arc<Mutex<Vec<Request>>>,
}

impl Scripted {
    fn answering(replies: impl IntoIterator<Item = Reply>) -> Scripted {
        Scripted {
            replies: Arc::new(Mutex::new(replies.into_iter().collect())),
            asked: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn asked(&self) -> Vec<Request> {
        self.asked.lock().expect("the log is not poisoned").clone()
    }
}

impl Responder for Scripted {
    fn respond(&self, request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        self.asked
            .lock()
            .expect("the log is not poisoned")
            .push(request.clone());
        let reply = self
            .replies
            .lock()
            .expect("the script is not poisoned")
            .pop_front()
            .expect("the script answers every request made of it");
        deltas.text(&reply.text);
        if let Some(reasoning) = &reply.reasoning {
            deltas.reasoning(reasoning);
        }
        Ok(reply)
    }
}

/// A front end that keeps the newest document and transcript it is handed, which is what a launch's
/// own record says once a run has stopped, the turn a run ended on, and the names the launch handed
/// it with those facts.
#[derive(Default)]
struct Kept {
    document: Option<SessionDocument>,
    transcript: Vec<Entry>,
    kept: Option<Turn>,
    loaded: Vec<String>,
}

impl Kept {
    fn ended_on(&self) -> Turn {
        self.kept
            .clone()
            .expect("the run hands back the turn it ended on")
    }
}

impl Turns for Kept {
    fn turn(&mut self, _ended: &Turn) {}

    fn kept(&mut self, kept: &Turn) {
        self.kept = Some(kept.clone());
    }

    fn facts(&mut self, handed: Handed<'_>) {
        self.document = Some(handed.document.clone());
        self.transcript = handed.transcript.to_vec();
        self.loaded = handed.loaded.names();
    }
}

/// A launch that answers through `script` and nothing else, on a config that names a run.
fn launch_answering(homes: &Homes, script: &Scripted, front_end: FrontEnd) -> Launch<Scripted> {
    let script = script.clone();
    Launch::answered_by(
        &homes.machine(),
        &naming_wire(String::from(UNANSWERED_ENDPOINT)),
        Enforcement::Unsafe,
        front_end,
        None,
        |_, _, _| script.clone(),
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run")
}

#[test]
fn a_launch_records_the_wire_the_model_and_the_boundary_it_runs() {
    let homes = Homes::new();
    let script = Scripted::answering([Reply::answer("triaged")]);

    let mut launch = launch_answering(&homes, &script, FrontEnd::Batch);
    let figure = launch.figure();

    assert_eq!(figure.wire, Wire::Anthropic, "the wire the config names");
    assert_eq!(figure.model, "claude-sonnet-4-5");
    assert_eq!(figure.enforcement, Enforcement::Unsafe);
    assert!(
        figure.session.is_dir(),
        "the session the launch holds is a directory already made: {}",
        figure.session.display()
    );

    let mut kept = Kept::default();
    match launch.drive("triage this", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(_) => {}
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    }

    let document = kept
        .document
        .expect("the launch hands the document its record holds");
    assert_eq!(document.wire, Wire::Anthropic);
    assert_eq!(document.model, "claude-sonnet-4-5");
    assert_eq!(document.enforcement, Enforcement::Unsafe);
    assert!(
        figure.session.join("session.json").exists(),
        "the record the launch wrote is in the session it named"
    );
}

#[test]
fn a_scripted_answer_ends_the_run_as_final_and_the_record_holds_it() {
    let homes = Homes::new();
    let script = Scripted::answering([Reply::answer("triaged").spent(Usage {
        input: Some(11),
        output: Some(7),
        ..Usage::default()
    })]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Batch);
    let figure = launch.figure();

    let mut kept = Kept::default();
    let outcome = match launch.drive("triage this", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    };

    assert_eq!(
        outcome.answer(),
        Some("triaged"),
        "the answer is the words the reply gave"
    );
    assert_eq!(
        outcome.stopped(),
        &Stop::Final,
        "a reply that asks for no call is the end of the run"
    );
    assert_eq!(
        script.asked().len(),
        1,
        "one prompt is one request to the wire"
    );

    let document = kept
        .document
        .expect("the launch hands the document its record holds");
    assert_eq!(document.turns, 1, "one reply is the one turn of the run");
    assert_eq!(document.stopped, State::Stopped(Stop::Final));
    assert_eq!(
        (document.usage.input, document.usage.output),
        (Some(11), Some(7)),
        "the record writes what the reply said it spent"
    );

    let transcript = read_transcript(&figure.session).expect("the record the launch wrote");
    let safe = SecretSet::default();
    assert_eq!(
        transcript.entries(),
        [
            Entry::Prompt {
                text: safe.redact("triage this"),
            },
            Entry::Turn { turn: 1 },
            Entry::Assistant {
                turn: 1,
                text: safe.redact("triaged"),
            },
            Entry::Stop { stop: Stop::Final },
        ],
        "the record holds the prompt, the turn it took, the answer and the stop, in that order"
    );
}

#[test]
fn the_prompt_after_the_first_sees_the_turn_it_ended_on() {
    let homes = Homes::new();
    let script = Scripted::answering([
        Reply::answer("the first answer"),
        Reply::answer("the second answer"),
    ]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Terminal);

    let mut first = Kept::default();
    let outcome = match launch.drive(
        "the first prompt",
        &[],
        &mut Quiet,
        &mut first,
        Cancel::new(),
    ) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    };
    assert_eq!(outcome.answer(), Some("the first answer"));
    let ended = first.ended_on();

    let mut second = Kept::default();
    let outcome = match launch.drive(
        "the second prompt",
        std::slice::from_ref(&ended),
        &mut Quiet,
        &mut second,
        Cancel::new(),
    ) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run after it stopped on its own: {}", refusal.message()),
    };
    assert_eq!(outcome.answer(), Some("the second answer"));

    let asked = script.asked();
    assert_eq!(asked.len(), 2, "each prompt in the conversation is one ask");
    assert!(
        asked[0].earlier().is_empty(),
        "the first prompt joins a conversation nothing has said yet: {:?}",
        asked[0].earlier()
    );
    let joined = asked[1].earlier();
    assert_eq!(
        joined,
        [ended],
        "the second ask carries the turn the first run ended on"
    );
    assert_eq!(joined[0].prompt, "the first prompt");
    assert_eq!(joined[0].text, "the first answer");
}

#[test]
fn a_skill_the_run_loaded_is_the_launchs_once_the_run_has_ended() {
    let homes = Homes::new();
    homes.user_skill("deploy", "ship the branch");
    let script = Scripted::answering([
        Reply::asking(
            "loading",
            [Call::new("call_1", SKILL, r#"{"name":"deploy"}"#)],
        ),
        Reply::answer("shipped"),
    ]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Terminal);

    let mut kept = Kept::default();
    match launch.drive("ship it", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(_) => {}
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    }

    assert_eq!(
        launch.loaded().names(),
        ["deploy"],
        "the name the model loaded is the launch's, so it outlives the run that loaded it"
    );
    assert_eq!(
        kept.loaded,
        ["deploy"],
        "and the launch hands the frame the same set it holds, not a copy of one"
    );
}

#[test]
fn the_prompt_after_a_load_starts_knowing_what_came_before() {
    let homes = Homes::new();
    homes.user_skill("deploy", "ship the branch");
    let loading = || {
        Reply::asking(
            "loading",
            [Call::new("call_1", SKILL, r#"{"name":"deploy"}"#)],
        )
    };
    let script = Scripted::answering([
        loading(),
        Reply::answer("shipped"),
        loading(),
        Reply::answer("shipped again"),
    ]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Terminal);

    let mut first = Kept::default();
    match launch.drive("ship it", &[], &mut Quiet, &mut first, Cancel::new()) {
        Ok(_) => {}
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    }
    let ended = first.ended_on();

    let mut second = Kept::default();
    match launch.drive(
        "ship it again",
        std::slice::from_ref(&ended),
        &mut Quiet,
        &mut second,
        Cancel::new(),
    ) {
        Ok(_) => {}
        Err(refusal) => panic!("the run after it stopped on its own: {}", refusal.message()),
    }

    let asked = script.asked();
    let loaded_in = |request: &Request| {
        request
            .history()
            .last()
            .and_then(|turn| turn.results.first())
            .map(|acted| acted.text.clone())
            .expect("the turn that asked for the skill carries what it answered")
    };
    let body = "Body of deploy.";
    assert!(
        loaded_in(&asked[1]).contains(body),
        "the first load hands the body over: {}",
        loaded_in(&asked[1])
    );
    let again = loaded_in(&asked[3]);
    assert!(
        again.contains("already_loaded") && !again.contains(body),
        "the prompt after the load starts knowing the body it was already handed, so a second \
         load of the same skill carries none of it: {again}"
    );
}

#[test]
fn a_launch_hands_the_transcript_its_record_wrote() {
    let homes = Homes::new();
    let script = Scripted::answering([Reply::answer("triaged")]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Terminal);
    let figure = launch.figure();

    let mut kept = Kept::default();
    match launch.drive("triage this", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(_) => {}
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    }

    let written = read_transcript(&figure.session).expect("the record the launch wrote");
    assert_eq!(
        serde_json::to_value(&kept.transcript).expect("a transcript is JSON"),
        serde_json::to_value(written.entries()).expect("a transcript is JSON"),
        "the transcript a launch hands is the one its record wrote"
    );
    let (Some(Entry::Prompt { .. }), Some(Entry::Stop { .. })) =
        (kept.transcript.first(), kept.transcript.last())
    else {
        panic!(
            "the transcript opens with the prompt it was asked and closes with the stop: {:?}",
            kept.transcript
        );
    };
}

#[test]
fn a_launch_that_names_no_session_mints_a_guid_of_its_own() {
    let homes = Homes::new();
    let first = Launch::open(
        &homes.machine(),
        &naming_wire(String::from(UNANSWERED_ENDPOINT)),
        Enforcement::Unsafe,
        FrontEnd::Terminal,
        None,
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");
    let second = Launch::open(
        &homes.machine(),
        &naming_wire(String::from(UNANSWERED_ENDPOINT)),
        Enforcement::Unsafe,
        FrontEnd::Terminal,
        None,
    )
    .ok()
    .expect("a second launch of the same config");

    assert_ne!(
        first.session().guid(),
        second.session().guid(),
        "a launch that names no session mints one, so two launches are two sessions"
    );
}

#[test]
fn a_launch_outlives_the_runs_it_drives() {
    let homes = Homes::new();
    let script = Scripted::answering([
        Reply::answer("the first answer"),
        Reply::answer("the second answer"),
    ]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Terminal);
    let session = launch.session();

    let first = match launch.drive("the first prompt", &[], &mut Quiet, &mut (), Cancel::new()) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    };
    let second = match launch.drive("the second prompt", &[], &mut Quiet, &mut (), Cancel::new()) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run after it stopped on its own: {}", refusal.message()),
    };

    assert_eq!(first.answer(), Some("the first answer"));
    assert_eq!(second.answer(), Some("the second answer"));
    assert_eq!(
        first.session(),
        second.session(),
        "the prompt after the first runs in the session the first opened"
    );
    assert_eq!(first.session(), session.guid());
    let documents = named(homes.state_home(), "session.json");
    assert_eq!(
        documents,
        [session.directory().join("session.json")],
        "one launch, one session, one record"
    );
    let transcript = read_transcript(session.directory()).expect("the record both runs wrote");
    let prompts: Vec<String> = transcript
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Prompt { text } => Some(text.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        prompts,
        ["the first prompt", "the second prompt"],
        "one record carries both prompts in the order they were asked"
    );

    std::fs::write(session.temp().join("a-command-wrote-this"), "words")
        .expect("a command's scratch in the session temp");
    launch.close();
    assert_eq!(
        std::fs::read_dir(session.temp())
            .expect("the temp is still on disk")
            .count(),
        0,
        "the conversation is over: the temp goes and the record stays"
    );
}

#[test]
fn a_launch_hands_the_words_and_skills_its_tools_were_built_from() {
    let homes = Homes::new();
    homes.user_skill("triage", "sort the tickets before you read them");
    let config = naming_wire(String::from(UNANSWERED_ENDPOINT));

    let launch = Launch::open(
        &homes.machine(),
        &config,
        Enforcement::Unsafe,
        FrontEnd::Batch,
        None,
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");

    let workspace = std::env::current_dir().expect("this process has a working directory");
    let described = Boundary::from_config(&config, &workspace, Enforcement::Unsafe)
        .expect("the boundary the launch was opened with holds")
        .description();
    assert_eq!(
        launch.bash_description(),
        described,
        "the words a run hands the model for `bash` are the boundary it was opened with, not a \
         rendering beside it"
    );

    let listed: Vec<String> = launch
        .listing()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(
        listed.contains(&String::from("triage")),
        "the catalog the launch reads is the one its tools were built from: {listed:?}"
    );
}

#[test]
fn a_launch_nobody_drove_leaves_no_session_behind() {
    let homes = Homes::new();
    let launch = Launch::open(
        &homes.machine(),
        &naming_wire(String::from(UNANSWERED_ENDPOINT)),
        Enforcement::Unsafe,
        FrontEnd::Batch,
        None,
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");
    let session = launch.figure().session;
    assert!(
        session.is_dir(),
        "the launch holds a session before it is discarded: {}",
        session.display()
    );

    launch.discard();

    assert!(
        !session.exists(),
        "a diagnostic records no run, so the session nobody wrote goes whole: {}",
        session.display()
    );
    let root = homes.state_home().join("sindri").join("sessions");
    assert_eq!(
        std::fs::read_dir(&root).expect("the sessions root").count(),
        0,
        "no reader finds a session nobody wrote in {}",
        root.display()
    );
}

#[test]
fn a_launch_the_sandbox_refuses_leaves_no_session_behind() {
    let homes = Homes::new();

    let Err(refusal) = Launch::open(
        &homes.machine_that_refuses(),
        &naming_wire(String::from(UNANSWERED_ENDPOINT)),
        Enforcement::Bubblewrap,
        FrontEnd::Batch,
        None,
    ) else {
        panic!("this machine cannot make the sandbox");
    };

    assert_eq!(
        refusal.code(),
        3,
        "the sandbox refusal is the setup refusal: {}",
        refusal.message()
    );
    let root = homes.state_home().join("sindri").join("sessions");
    let kept: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("the sessions root the launch made")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    assert!(
        kept.is_empty(),
        "the session is made before the proof, so a refusal takes it away again: {kept:?}"
    );
}

/// An opener that reports a target opened and opens it nowhere.
#[derive(Debug)]
struct OpensNowhere;

impl Opener for OpensNowhere {
    fn open(&self, _opening: &Opening) -> Handover {
        Handover::Opened
    }
}

fn every_registered_tool_is_answered(launch: &Launch) {
    for tool in launch.tools() {
        assert!(
            launch.executor().answers(tool.name()),
            "the front end registers `{}` and the launch's executor cannot run it",
            tool.name()
        );
    }
}

#[test]
fn every_tool_a_terminal_launch_registers_is_a_tool_its_own_executor_answers() {
    let homes = Homes::new();
    homes.user_skill("deploy", "ship the branch");

    let launch = Launch::open(
        &homes.machine(),
        &naming_wire(String::from(UNANSWERED_ENDPOINT)),
        Enforcement::Unsafe,
        FrontEnd::Terminal,
        Some(Arc::new(OpensNowhere)),
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");

    assert!(
        launch.tools().iter().any(|tool| tool.name() == OPEN),
        "a terminal launch registers `open`, the tool a front end has shipped without an arm"
    );

    every_registered_tool_is_answered(&launch);
}

#[test]
fn every_tool_a_batch_launch_registers_is_a_tool_its_own_executor_answers() {
    let homes = Homes::new();
    homes.user_skill("deploy", "ship the branch");

    let launch = Launch::open(
        &homes.machine(),
        &naming_wire(String::from(UNANSWERED_ENDPOINT)),
        Enforcement::Unsafe,
        FrontEnd::Batch,
        None,
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");

    assert!(
        launch.tools().iter().any(|tool| tool.name() == SKILL),
        "the skill this test planted gives the batch set a second tool to walk"
    );

    every_registered_tool_is_answered(&launch);
}
