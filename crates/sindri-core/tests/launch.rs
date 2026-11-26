use std::collections::{BTreeMap, VecDeque};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sindri_core::config::Config;
use sindri_core::first_run::FirstRun;
use sindri_core::launch::{Handed, Launch, Machine, Turns};
use sindri_core::open::{Handover, Opener, Opening};
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::provider::{Deltas, Failure, Usage};
use sindri_core::records::{Entry, SessionDocument, Told, read_status, read_transcript};
use sindri_core::redact::SecretSet;
use sindri_core::run::{Call, Cancel, Reply, Request, Responder, State, Stop, Turn};
use sindri_core::tools::{FrontEnd, OPEN, SKILL};
use sindri_core::wire::Wire;
use tempfile::TempDir;

const RESTRICTED: &str = "if [ \"${1:-}\" = --version ]; then\n  \
                          echo 'bubblewrap 0.12.0 (stand-in)'\n  exit 0\nfi\n\
                          echo 'bwrap: setting up uid map: Permission denied' >&2\nexit 1";

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

    fn machine_that_refuses(&self) -> Machine<'_> {
        let bwrap = self.at("bin").join("bwrap");
        std::fs::create_dir_all(self.at("bin")).expect("a machine directory");
        std::fs::write(&bwrap, format!("#!/bin/sh\n{RESTRICTED}\n")).expect("a stand-in bwrap");
        std::fs::set_permissions(&bwrap, Permissions::from_mode(0o755))
            .expect("the stand-in is executable");
        Machine::new(&self.environment, bwrap)
    }

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

    let Err(refusal) = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .open(
            &homes.machine(),
            &Config::default(),
            Enforcement::Bubblewrap,
            FrontEnd::Batch,
            None,
        )
    else {
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

    let Err(refusal) = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .answered_by(
            &machine,
            &naming_wire(String::from(UNANSWERED_ENDPOINT)),
            Enforcement::Bubblewrap,
            FrontEnd::Batch,
            None,
            |_, _, _| script.clone(),
        )
    else {
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

struct Quiet;

impl Deltas for Quiet {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

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

#[derive(Default)]
struct Kept {
    document: Option<SessionDocument>,
    transcript: Vec<Entry>,
    kept: Option<Turn>,
    loaded: Vec<String>,
    turns: Vec<u32>,
    told: Vec<Told>,
}

impl Kept {
    fn ended_on(&self) -> Turn {
        self.kept
            .clone()
            .expect("the run hands back the turn it ended on")
    }
}

impl Turns for Kept {
    fn turn(&mut self, ended: &Turn) {
        self.turns.push(ended.turn);
    }

    fn told(&mut self, told: Told) {
        self.told.push(told);
    }

    fn kept(&mut self, kept: &Turn) {
        self.kept = Some(kept.clone());
    }

    fn facts(&mut self, handed: Handed<'_>) {
        self.document = Some(handed.document.clone());
        self.transcript = handed.transcript.to_vec();
        self.loaded = handed.loaded.names();
    }
}

fn launch_answering(homes: &Homes, script: &Scripted, front_end: FrontEnd) -> Launch<Scripted> {
    let script = script.clone();
    FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .answered_by(
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

fn statuses_written(directory: &Path) -> Vec<(&'static str, u32, String)> {
    read_status(directory)
        .expect("the statuses the launch wrote")
        .iter()
        .map(|status| {
            (
                status.phase().word(),
                status.turn(),
                status.message().to_string(),
            )
        })
        .collect()
}

#[test]
fn a_driven_launch_writes_the_status_its_front_end_was_told() {
    let homes = Homes::new();
    let script = Scripted::answering([Reply::answer("triaged")]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Batch);
    let figure = launch.figure();

    let mut kept = Kept::default();
    match launch.drive("triage this", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(_) => {}
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    }

    assert_eq!(
        statuses_written(&figure.session),
        [
            ("working", 1, String::from("asking the model")),
            ("done", 1, String::from("answered")),
        ],
        "a launch leaves one status for each phase its run passed through"
    );
    assert_eq!(
        kept.told,
        [
            Told::Asking { turn: 1 },
            Told::Stopped {
                turn: 1,
                stop: Stop::Final,
            },
        ],
        "the telling the record wrote is the one the front end was handed"
    );
}

#[test]
fn a_turn_that_asks_for_a_call_moves_the_turn_the_document_names() {
    let homes = Homes::new();
    let script = Scripted::answering([
        Reply::asking(
            "running it",
            [Call::new("call_1", "bash", r#"{"command":"true"}"#)],
        ),
        Reply::answer("it ran"),
    ]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Batch);
    let figure = launch.figure();

    let mut kept = Kept::default();
    match launch.drive("run it", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(_) => {}
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    }

    assert_eq!(
        statuses_written(&figure.session),
        [
            ("working", 1, String::from("asking the model")),
            ("blocked", 1, String::from("waiting for a command")),
            ("working", 2, String::from("asking the model")),
            ("done", 2, String::from("answered")),
        ],
        "a reader of the documents sees the run ask, wait on its call, ask again and stop"
    );
}

#[test]
fn a_second_prompt_numbers_the_documents_from_its_own_first_turn() {
    let homes = Homes::new();
    let script = Scripted::answering([Reply::answer("the first"), Reply::answer("the second")]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Terminal);
    let figure = launch.figure();

    let mut first = Kept::default();
    match launch.drive(
        "the first prompt",
        &[],
        &mut Quiet,
        &mut first,
        Cancel::new(),
    ) {
        Ok(_) => {}
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    }
    let ended = first.ended_on();

    let mut second = Kept::default();
    match launch.drive(
        "the second prompt",
        std::slice::from_ref(&ended),
        &mut Quiet,
        &mut second,
        Cancel::new(),
    ) {
        Ok(_) => {}
        Err(refusal) => panic!("the run after it stopped on its own: {}", refusal.message()),
    }

    let written = statuses_written(&figure.session);
    assert_eq!(
        &written[2..],
        [
            ("working", 1, String::from("asking the model")),
            ("done", 1, String::from("answered")),
        ],
        "the session's document has counted the turn before it, and the run counts its own: \
         {written:?}"
    );
}

#[test]
fn a_run_that_stopped_on_an_error_leaves_its_last_document_saying_error() {
    let homes = Homes::new();
    let script = Scripted::answering([Reply::asking(
        "reading it",
        [Call::new("call_1", "read", r#"{"path":"notes.md"}"#)],
    )]);
    let mut launch = launch_answering(&homes, &script, FrontEnd::Batch);
    let figure = launch.figure();

    let mut kept = Kept::default();
    let outcome = match launch.drive("read it", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    };

    assert_eq!(
        statuses_written(&figure.session),
        [
            ("working", 1, String::from("asking the model")),
            ("blocked", 1, String::from("waiting for a command")),
            ("error", 1, String::from("failed")),
        ],
        "a run whose call failed waited on it, and leaves its last document saying error: {}",
        outcome.stopped()
    );
    assert!(
        matches!(
            kept.told.last(),
            Some(Told::Stopped {
                stop: Stop::Error(_),
                ..
            })
        ),
        "and the front end is told the same error: {:?}",
        kept.told
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
fn a_skill_a_run_loads_outlives_the_run_that_loaded_it() {
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
fn the_prompt_after_a_load_gets_the_skill_already_loaded() {
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
    let first = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .open(
            &homes.machine(),
            &naming_wire(String::from(UNANSWERED_ENDPOINT)),
            Enforcement::Unsafe,
            FrontEnd::Terminal,
            None,
        )
        .ok()
        .expect("a config naming a wire, a model and a key names a run");
    let second = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .open(
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

    let launch = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .open(
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
    let launch = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .open(
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

    let Err(refusal) = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .open(
            &homes.machine_that_refuses(),
            &naming_wire(String::from(UNANSWERED_ENDPOINT)),
            Enforcement::Bubblewrap,
            FrontEnd::Batch,
            None,
        )
    else {
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

    let launch = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .open(
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

    let launch = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .open(
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

fn turns_written(directory: &Path) -> Vec<u32> {
    read_transcript(directory)
        .expect("the record the launch wrote")
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Turn { turn } => Some(*turn),
            _ => None,
        })
        .collect()
}

#[test]
fn a_launch_stopped_by_the_turn_cap_writes_the_turn_it_took_once() {
    let homes = Homes::new();
    homes.user_skill("deploy", "ship the branch");
    let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
    config.max_turns = Some(1);
    let script = Scripted::answering([
        Reply::asking(
            "loading",
            [Call::new("call_1", SKILL, r#"{"name":"deploy"}"#)],
        ),
        Reply::answer("never asked"),
    ]);
    let script = script.clone();
    let mut launch = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .answered_by(
            &homes.machine(),
            &config,
            Enforcement::Unsafe,
            FrontEnd::Batch,
            None,
            |_, _, _| script.clone(),
        )
        .ok()
        .expect("a config naming a wire, a model and a key names a run");
    let figure = launch.figure();

    let mut kept = Kept::default();
    let outcome = match launch.drive("ship it", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    };

    assert_eq!(
        outcome.stopped(),
        &Stop::MaxTurns,
        "one turn is the cap this launch was given"
    );
    assert_eq!(
        turns_written(&figure.session),
        [1],
        "the one turn it took is written once, not once more for the pass that took none"
    );
}

#[test]
fn a_launch_hands_the_front_end_every_turn_it_took() {
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
    let figure = launch.figure();

    let mut kept = Kept::default();
    match launch.drive("ship it", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(_) => {}
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    }

    assert_eq!(
        kept.turns,
        [1, 2],
        "the front end sees each turn the run took, in order"
    );
    assert_eq!(
        turns_written(&figure.session),
        [1, 2],
        "the record writes each turn once, in the order the loop took them"
    );
}

#[test]
fn two_prompts_replay_every_completed_tool_turn_on_each_wire() {
    use sindri_core::asking::Asking;
    use sindri_core::conversation::Conversation;
    use sindri_core::provider::transport::{Canned, Replay};
    use sindri_core::provider::{Client, Route};

    #[derive(Default)]
    struct Retaining(Conversation);
    impl Turns for Retaining {
        fn turn(&mut self, _: &Turn) {}
        fn retained(&mut self, turns: &[Turn]) {
            for turn in turns {
                self.0.join(turn.clone());
            }
        }
    }

    for (wire, model, route, tool, answer) in [
        (
            Wire::Anthropic,
            "claude-sonnet-4-6",
            Route::Chat,
            include_str!("fixtures/providers/anthropic_thinking_tool_use.sse"),
            include_str!("fixtures/providers/anthropic_stream.sse"),
        ),
        (
            Wire::Openai,
            "gpt-4o",
            Route::Chat,
            include_str!("fixtures/providers/openai_chat_tool_use.sse"),
            include_str!("fixtures/providers/copilot_chat_stream.sse"),
        ),
        (
            Wire::Openai,
            "gpt-5",
            Route::Responses,
            include_str!("fixtures/providers/openai_responses_tool_use.sse"),
            include_str!("fixtures/providers/openai_responses_stream.sse"),
        ),
        (
            Wire::Copilot,
            "gpt-5",
            Route::Chat,
            include_str!("fixtures/providers/openai_chat_tool_use.sse"),
            include_str!("fixtures/providers/copilot_chat_stream.sse"),
        ),
    ] {
        let homes = Homes::new();
        let answer = if route == Route::Responses {
            answer.replace(
                r#""id":"rs_01","type":"reasoning","summary""#,
                r#""id":"rs_01","type":"reasoning","encrypted_content":"FINAL_OPAQUE","summary""#,
            )
        } else {
            answer.to_owned()
        };
        let executed = homes.at("executed");
        let command = format!(
            "printf x >> {}; printf REPLAY_; printf EVIDENCE_ONLY",
            executed.display()
        );
        let tool = if route == Route::Responses {
            let tool = tool.replace(r#""output_index":0"#, r#""output_index":1"#);
            let tool = format!(
                "event: response.output_item.done\ndata: {{\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{{\"type\":\"reasoning\",\"id\":\"rs_tool_01\",\"summary\":[],\"encrypted_content\":\"OPAQUE_01\"}}}}\n\n{tool}"
            );
            tool.replace(
                r#""output":[{"type":"function_call""#,
                r#""output":[{"type":"reasoning","id":"rs_tool_01","summary":[],"encrypted_content":"OPAQUE_01"},{"type":"function_call""#,
            )
        } else {
            tool.to_owned()
        };
        let tool = tool.replace(r#"ls\""#, &format!(r#"{command}\""#));
        let tool = if route == Route::Chat && wire != Wire::Anthropic {
            tool.replace(r#""content":null"#, r#""content":"Inspecting evidence""#)
        } else {
            tool
        };

        let replay = Replay::new([
            Canned::stream(Box::leak(tool.clone().into_boxed_str())),
            Canned::stream(Box::leak(tool.replace("_01", "_02").into_boxed_str())),
            Canned::stream(Box::leak(answer.clone().into_boxed_str())),
            Canned::stream(Box::leak(answer.clone().into_boxed_str())),
        ]);
        let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
        config.provider = Some(wire);
        config.model = Some(model.into());
        config.thinking = Some(wire == Wire::Anthropic);
        config.context_window = Some(1_000_000);

        let client = Client::over(replay.clone(), SecretSet::default());
        let mut launch = FirstRun::resolve(&homes.environment, Config::default())
            .unwrap()
            .answered_by(
                &homes.machine(),
                &config,
                Enforcement::Unsafe,
                FrontEnd::Terminal,
                None,
                |_, ask, tools| Asking::new(client.clone(), ask.route(route), tools),
            )
            .ok()
            .unwrap();
        let prompt = format!("Inspect {}", homes.root.path().display());
        let mut retaining = Retaining::default();
        let first = launch
            .drive(&prompt, &[], &mut Quiet, &mut retaining, Cancel::new())
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        assert!(!first.answer().unwrap().contains("REPLAY_EVIDENCE_ONLY"));
        let earlier = retaining.0.turns().to_vec();
        launch
            .drive(
                "Use the evidence",
                &earlier,
                &mut Quiet,
                &mut retaining,
                Cancel::new(),
            )
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        assert_eq!(std::fs::read_to_string(&executed).unwrap(), "xx");
        let requests = replay.requests();
        assert_eq!(
            requests.len(),
            4,
            "completed tools must not run again for {wire:?}/{route:?}: {}",
            first.stopped()
        );
        let last: serde_json::Value = serde_json::from_str(&requests[3].body).unwrap();
        let previous: serde_json::Value = serde_json::from_str(&requests[2].body).unwrap();
        let field = if route == Route::Responses {
            "input"
        } else {
            "messages"
        };
        if route == Route::Responses {
            for (request, expected) in [(&requests[1], 1), (&requests[2], 2), (&requests[3], 2)] {
                let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
                assert!(body.get("store").is_none());
                assert!(body.get("previous_response_id").is_none());
                let items = body["input"].as_array().unwrap();
                for suffix in ["01", "02"].into_iter().take(expected) {
                    let position = items
                        .iter()
                        .position(|item| item["id"] == format!("rs_tool_{suffix}"))
                        .expect("original reasoning item");
                    assert_eq!(
                        items[position],
                        serde_json::json!({"type":"reasoning","id":format!("rs_tool_{suffix}"),"summary":[],"encrypted_content":format!("OPAQUE_{suffix}")})
                    );
                    assert_eq!(items[position + 1]["id"], format!("fc_{suffix}"));
                    assert_eq!(items[position + 1]["call_id"], format!("call_{suffix}"));
                    assert_eq!(items[position + 2]["type"], "function_call_output");
                    assert_eq!(items[position + 2]["call_id"], format!("call_{suffix}"));
                }
            }
            assert!(
                last["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["id"] == "msg_01"),
                "{last}"
            );
        }
        let messages = last[field].as_array().unwrap();
        let prefix = previous[field].as_array().unwrap();
        assert!(messages.len() > prefix.len());
        if route == Route::Chat && wire != Wire::Anthropic {
            assert_eq!(requests[3].body.matches("Inspecting evidence").count(), 2);
        }

        if wire == Wire::Anthropic {
            for request in &requests[1..] {
                let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
                let messages = body["messages"].as_array().unwrap();
                let assistant = messages
                    .iter()
                    .position(|m| m["role"] == "assistant")
                    .unwrap();
                assert_eq!(
                    messages[assistant]["content"],
                    serde_json::json!([
                        {"type":"thinking","thinking":"Private plan","signature":"opaque_SIG_+/=01"},
                        {"type":"redacted_thinking","data":"opaque_REDACTED_+/=01"},
                        {"type":"text","text":"Inspecting evidence"},
                        {"type":"tool_use","id":"toolu_01","name":"bash","input":{"command":command}}
                    ])
                );
                assert_eq!(
                    messages[assistant + 1]["content"][0]["tool_use_id"],
                    "toolu_01"
                );
                if messages.len() > assistant + 2 && messages[assistant + 2]["role"] == "assistant"
                {
                    let second = &messages[assistant + 2]["content"];
                    assert_eq!(second[0]["signature"], "opaque_SIG_+/=01");
                    assert_eq!(second[1]["data"], "opaque_REDACTED_+/=01");
                    assert_eq!(second[3]["id"], "toolu_02");
                    assert_eq!(
                        messages[assistant + 3]["content"][0]["tool_use_id"],
                        "toolu_02"
                    );
                }
            }
        }

        let id = if wire == Wire::Anthropic {
            "toolu"
        } else {
            "call"
        };
        for suffix in ["01", "02"] {
            assert_eq!(
                requests[3].body.matches(&format!("{id}_{suffix}")).count(),
                2
            );
        }
        assert_eq!(
            &messages[..prefix.len()],
            prefix,
            "the request prefix stays stable for {wire:?}"
        );
        assert_eq!(
            requests[3].body.matches("REPLAY_EVIDENCE_ONLY").count(),
            2,
            "both tool results survive for {wire:?}: {}",
            requests[3].body
        );
        let generation = retaining.0.generation();
        let late = retaining.0.turns().to_vec();
        retaining.0.reset(launch.loaded());
        for turn in late {
            retaining.0.join_for(generation, turn);
        }
        let earlier = retaining.0.turns().to_vec();
        launch
            .drive(
                "Fresh conversation",
                &earlier,
                &mut Quiet,
                &mut retaining,
                Cancel::new(),
            )
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        let requests = replay.requests();
        for sentinel in [
            "REPLAY_EVIDENCE_ONLY",
            "OPAQUE_01",
            "OPAQUE_02",
            "FINAL_OPAQUE",
            "opaque_SIG",
            "opaque_REDACTED",
            "Use the evidence",
        ] {
            assert!(!requests[4].body.contains(sentinel), "{wire:?}: {sentinel}");
        }
        assert_eq!(std::fs::read_to_string(&executed).unwrap(), "xx");
    }
}

#[test]
fn a_responses_launch_refuses_a_call_whose_original_item_never_completed() {
    use sindri_core::asking::Asking;
    use sindri_core::provider::transport::{Canned, Replay};
    use sindri_core::provider::{Client, Route};

    let homes = Homes::new();
    let tool = include_str!("fixtures/providers/openai_responses_tool_use.sse");
    let tool = tool
        .lines()
        .filter(|line| !line.contains("output_item.done") && !line.contains("response.completed"))
        .collect::<Vec<_>>()
        .join("\n");
    let tool = format!(
        "{tool}\n\nevent: response.completed\ndata: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp_tool\",\"status\":\"completed\",\"model\":\"gpt-5\",\"output\":[]}}}}\n\n"
    );
    let replay = Replay::new([Canned::stream(Box::leak(tool.into_boxed_str()))]);
    let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
    config.provider = Some(Wire::Openai);
    config.model = Some("gpt-5".into());
    let client = Client::over(replay.clone(), SecretSet::default());
    let mut launch = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .answered_by(
            &homes.machine(),
            &config,
            Enforcement::Unsafe,
            FrontEnd::Terminal,
            None,
            |_, ask, tools| Asking::new(client.clone(), ask.route(Route::Responses), tools),
        )
        .ok()
        .unwrap();
    let outcome = launch
        .drive(
            "Inspect",
            &[],
            &mut Quiet,
            &mut Kept::default(),
            Cancel::new(),
        )
        .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
    assert!(matches!(outcome.stopped(), Stop::Error(_)));
    assert_eq!(replay.requests().len(), 1);
}

#[test]
fn two_prompts_keep_activated_skill_snapshots_on_each_wire() {
    use sindri_core::asking::Asking;
    use sindri_core::conversation::Conversation;
    use sindri_core::provider::transport::{Canned, Replay};
    use sindri_core::provider::{Client, Route};

    #[derive(Default)]
    struct Retaining(Conversation);
    impl Turns for Retaining {
        fn turn(&mut self, _: &Turn) {}
        fn retained(&mut self, turns: &[Turn]) {
            for turn in turns {
                self.0.join(turn.clone());
            }
        }
    }

    for (wire, model, route, tool, answer) in [
        (
            Wire::Anthropic,
            "claude-sonnet-4-6",
            Route::Chat,
            include_str!("fixtures/providers/anthropic_tool_use.sse"),
            include_str!("fixtures/providers/anthropic_stream.sse"),
        ),
        (
            Wire::Openai,
            "gpt-4o",
            Route::Chat,
            include_str!("fixtures/providers/openai_chat_tool_use.sse"),
            include_str!("fixtures/providers/copilot_chat_stream.sse"),
        ),
        (
            Wire::Openai,
            "gpt-5",
            Route::Responses,
            include_str!("fixtures/providers/openai_responses_tool_use.sse"),
            include_str!("fixtures/providers/openai_responses_stream.sse"),
        ),
        (
            Wire::Copilot,
            "gpt-5",
            Route::Chat,
            include_str!("fixtures/providers/openai_chat_tool_use.sse"),
            include_str!("fixtures/providers/copilot_chat_stream.sse"),
        ),
    ] {
        let homes = Homes::new();
        homes.user_skill("deploy", "ship the branch");
        let tool = tool
            .replace("bash", "skill")
            .replace("command", "name")
            .replace(r#"\"ls\""#, r#"\"deploy\""#);

        let replay = Replay::new([
            Canned::stream(Box::leak(tool.clone().into_boxed_str())),
            Canned::stream(answer),
            Canned::stream(answer),
        ]);
        let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
        config.provider = Some(wire);
        config.model = Some(model.into());
        let client = Client::over(replay.clone(), SecretSet::default());
        let mut launch = FirstRun::resolve(&homes.environment, Config::default())
            .unwrap()
            .answered_by(
                &homes.machine(),
                &config,
                Enforcement::Unsafe,
                FrontEnd::Terminal,
                None,
                |_, ask, tools| Asking::new(client.clone(), ask.route(route), tools),
            )
            .ok()
            .unwrap();
        let prompt = format!("Inspect {}", homes.root.path().display());
        let mut retaining = Retaining::default();
        let first = launch
            .drive(&prompt, &[], &mut Quiet, &mut retaining, Cancel::new())
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        assert!(
            !first
                .answer()
                .unwrap_or_else(|| panic!("{wire:?}: {}", first.stopped()))
                .contains("Body of deploy.")
        );
        let protected = launch.loaded().protected();
        std::fs::write(
            homes.at("home/.agents/skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: changed\n---\nREPLACED_BODY",
        )
        .unwrap();
        assert_eq!(launch.loaded().protected(), protected);
        let earlier = retaining.0.turns().to_vec();
        launch
            .drive(
                "Use the evidence",
                &earlier,
                &mut Quiet,
                &mut retaining,
                Cancel::new(),
            )
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        let requests = replay.requests();
        assert_eq!(requests.len(), 3);
        let last: serde_json::Value = serde_json::from_str(&requests[2].body).unwrap();
        let previous: serde_json::Value = serde_json::from_str(&requests[1].body).unwrap();
        let field = if route == Route::Responses {
            "input"
        } else {
            "messages"
        };
        let messages = last[field].as_array().unwrap();
        let prefix = previous[field].as_array().unwrap();
        assert!(messages.len() > prefix.len());
        assert_eq!(
            &messages[..prefix.len()],
            prefix,
            "the request prefix stays stable for {wire:?}"
        );
        assert_eq!(
            requests[2].body.matches("Body of deploy.").count(),
            1,
            "the exact skill body survives once for {wire:?}: {}",
            requests[2].body
        );
    }
}

#[test]
fn human_activation_delivers_and_retains_skill_snapshots_on_each_wire() {
    use sindri_core::asking::Asking;
    use sindri_core::conversation::Conversation;
    use sindri_core::provider::transport::{Canned, Replay};
    use sindri_core::provider::{Client, Route};

    #[derive(Default)]
    struct Retaining(Conversation);
    impl Turns for Retaining {
        fn turn(&mut self, _: &Turn) {}
        fn retained(&mut self, turns: &[Turn]) {
            for turn in turns {
                self.0.join(turn.clone());
            }
        }
    }

    for (wire, model, route, tool, answer) in [
        (
            Wire::Anthropic,
            "claude-sonnet-4-6",
            Route::Chat,
            include_str!("fixtures/providers/anthropic_tool_use.sse"),
            include_str!("fixtures/providers/anthropic_stream.sse"),
        ),
        (
            Wire::Openai,
            "gpt-4o",
            Route::Chat,
            include_str!("fixtures/providers/openai_chat_tool_use.sse"),
            include_str!("fixtures/providers/copilot_chat_stream.sse"),
        ),
        (
            Wire::Openai,
            "gpt-5",
            Route::Responses,
            include_str!("fixtures/providers/openai_responses_tool_use.sse"),
            include_str!("fixtures/providers/openai_responses_stream.sse"),
        ),
        (
            Wire::Copilot,
            "gpt-5",
            Route::Chat,
            include_str!("fixtures/providers/openai_chat_tool_use.sse"),
            include_str!("fixtures/providers/copilot_chat_stream.sse"),
        ),
    ] {
        let homes = Homes::new();
        homes.user_skill("deploy", "ship the branch");
        let _tool = tool
            .replace("bash", "skill")
            .replace("command", "name")
            .replace(r#"\"ls\""#, r#"\"deploy\""#);

        let replay = Replay::new([
            Canned::stream(answer),
            Canned::stream(answer),
            Canned::stream(answer),
        ]);
        let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
        config.provider = Some(wire);
        config.model = Some(model.into());
        let client = Client::over(replay.clone(), SecretSet::default());
        let mut launch = FirstRun::resolve(&homes.environment, Config::default())
            .unwrap()
            .answered_by(
                &homes.machine(),
                &config,
                Enforcement::Unsafe,
                FrontEnd::Terminal,
                None,
                |_, ask, tools| Asking::new(client.clone(), ask.route(route), tools),
            )
            .ok()
            .unwrap();
        let catalog =
            sindri_core::skills::Catalog::discover(homes.root.path(), Some(&homes.at("home")));
        launch.loaded().activate(catalog.get("deploy").unwrap());
        let prompt = format!("Inspect {}", homes.root.path().display());
        let mut retaining = Retaining::default();
        let first = launch
            .drive(&prompt, &[], &mut Quiet, &mut retaining, Cancel::new())
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        assert!(
            !first
                .answer()
                .unwrap_or_else(|| panic!("{wire:?}: {}", first.stopped()))
                .contains("Body of deploy.")
        );
        let protected = launch.loaded().protected();
        std::fs::write(
            homes.at("home/.agents/skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: changed\n---\nREPLACED_BODY",
        )
        .unwrap();
        assert_eq!(launch.loaded().protected(), protected);
        let earlier = retaining.0.turns().to_vec();
        launch
            .drive(
                "Use the evidence",
                &earlier,
                &mut Quiet,
                &mut retaining,
                Cancel::new(),
            )
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        let requests = replay.requests();
        assert_eq!(requests.len(), 2);
        let last: serde_json::Value = serde_json::from_str(&requests[1].body).unwrap();
        let previous: serde_json::Value = serde_json::from_str(&requests[0].body).unwrap();
        let field = if route == Route::Responses {
            "input"
        } else {
            "messages"
        };
        let messages = last[field].as_array().unwrap();
        let prefix = previous[field].as_array().unwrap();
        assert!(messages.len() > prefix.len());
        assert_eq!(
            &messages[..prefix.len()],
            prefix,
            "the request prefix stays stable for {wire:?}"
        );
        assert_eq!(
            requests[1].body.matches("Body of deploy.").count(),
            1,
            "the exact skill body survives once for {wire:?}: {}",
            requests[1].body
        );
        let generation = retaining.0.generation();
        let superseded = launch.loaded().scoped();
        retaining.0.reset(launch.loaded());
        superseded.activate(catalog.get("deploy").unwrap());
        retaining
            .0
            .join_for(generation, Turn::answered(1, "OLD_PROMPT", "OLD_ARTIFACT"));
        let earlier = retaining.0.turns().to_vec();
        launch
            .drive(
                "Independent prompt",
                &earlier,
                &mut Quiet,
                &mut retaining,
                Cancel::new(),
            )
            .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
        let requests = replay.requests();
        for sentinel in [
            "Body of deploy.",
            "OLD_PROMPT",
            "OLD_ARTIFACT",
            "Use the evidence",
        ] {
            assert!(!requests[2].body.contains(sentinel), "{wire:?}: {sentinel}");
        }
        assert!(launch.loaded().names().is_empty());
    }
}

#[test]
fn interrupted_parallel_work_replays_truthful_outcomes_without_reexecution() {
    use sindri_core::asking::Asking;
    use sindri_core::conversation::Conversation;
    use sindri_core::provider::transport::{Canned, Replay};
    use sindri_core::provider::{Client, Route};

    #[derive(Default)]
    struct Retaining(Conversation);
    impl Turns for Retaining {
        fn turn(&mut self, _: &Turn) {}
        fn retained(&mut self, turns: &[Turn]) {
            for turn in turns {
                self.0.join(turn.clone());
            }
        }
    }

    for (wire, model, route, answer) in [
        (
            Wire::Anthropic,
            "claude-sonnet-4-6",
            Route::Chat,
            include_str!("fixtures/providers/anthropic_stream.sse"),
        ),
        (
            Wire::Openai,
            "gpt-4o",
            Route::Chat,
            include_str!("fixtures/providers/copilot_chat_stream.sse"),
        ),
        (
            Wire::Openai,
            "gpt-5",
            Route::Responses,
            include_str!("fixtures/providers/openai_responses_stream.sse"),
        ),
        (
            Wire::Copilot,
            "gpt-5",
            Route::Chat,
            include_str!("fixtures/providers/copilot_chat_stream.sse"),
        ),
    ] {
        for stop in ["error", "budget", "max_turns", "cancelled", "failed"] {
            let homes = Homes::new();
            let executed = homes.at("executed");
            let command = format!(
                "printf x >> {}; printf PARTIAL_EVIDENCE{}",
                executed.display(),
                if stop == "cancelled" {
                    "; sleep 10"
                } else {
                    ""
                }
            );
            let second = if stop == "error" {
                "{\"command\":".to_owned()
            } else {
                serde_json::json!({"command": format!("printf y >> {}", executed.display())})
                    .to_string()
            };
            let calls = serde_json::json!([
                {"index":0,"id":"call_done","type":"function","function":{"name":"bash","arguments":serde_json::json!({"command": if stop == "failed" { format!("{command}; exit 7") } else { command }}).to_string()}},
                {"index":1,"id":"call_pending","type":"function","function":{"name":"bash","arguments":second}}
            ]);
            let mut tool = format!(
                "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
                serde_json::json!({"id":"chatcmpl-partial","object":"chat.completion.chunk","created":1760000001,"model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":calls},"finish_reason":null}]}),
                serde_json::json!({"id":"chatcmpl-partial","object":"chat.completion.chunk","created":1760000001,"model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]})
            );
            if wire == Wire::Anthropic || route == Route::Responses {
                let fixture = if wire == Wire::Anthropic {
                    include_str!("fixtures/providers/anthropic_tool_use.sse")
                } else {
                    include_str!("fixtures/providers/openai_responses_tool_use.sse")
                };
                let mut events = Vec::new();
                for block in fixture.trim().split("\n\n") {
                    let mut lines = block.lines();
                    let event = lines.next().unwrap();
                    let data = lines.next().unwrap().strip_prefix("data: ").unwrap();
                    let value: serde_json::Value = serde_json::from_str(data).unwrap();
                    if event.contains("content_block")
                        || event.contains("response.output_item")
                        || event.contains("response.function_call")
                    {
                        for (index, call) in calls.as_array().unwrap().iter().enumerate() {
                            let mut value = value.clone();
                            let arguments = call["function"]["arguments"].as_str().unwrap();
                            let id = call["id"].as_str().unwrap();
                            if wire == Wire::Anthropic {
                                value["index"] = index.into();
                                if let Some(block) = value.get_mut("content_block") {
                                    block["id"] = id.into();
                                }
                                if let Some(delta) = value.get_mut("delta") {
                                    delta["partial_json"] =
                                        if data.contains("ls") { "" } else { arguments }.into();
                                }
                            } else {
                                if value.get("output_index").is_some() {
                                    value["output_index"] = index.into();
                                }
                                if value.get("item_id").is_some() {
                                    value["item_id"] = format!("fc_{index}").into();
                                }
                                if value.get("delta").is_some() {
                                    value["delta"] = arguments.into();
                                }
                                if value.get("arguments").is_some() {
                                    value["arguments"] = arguments.into();
                                }
                                if let Some(item) = value.get_mut("item") {
                                    item["id"] = format!("fc_{index}").into();
                                    item["call_id"] = id.into();
                                    if !item["arguments"].as_str().unwrap().is_empty() {
                                        item["arguments"] = arguments.into();
                                    }
                                }
                            }
                            events.push(format!("{event}\ndata: {value}\n\n"));
                        }
                    } else {
                        let mut value = value;
                        if let Some(response) = value.get_mut("response") {
                            response["output"] = serde_json::Value::Array(calls.as_array().unwrap().iter().enumerate().map(|(index, call)| serde_json::json!({"type":"function_call","id":format!("fc_{index}"),"call_id":call["id"],"name":"bash","arguments":call["function"]["arguments"]})).collect());
                        }
                        events.push(format!("{event}\ndata: {value}\n\n"));
                    }
                }
                tool = events.concat();
            }
            let replay = Replay::new([
                Canned::stream(Box::leak(tool.into_boxed_str())),
                Canned::stream(answer),
            ]);
            let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
            config.provider = Some(wire);
            config.model = Some(model.into());
            config.max_turns = Some(1);
            if stop == "budget" {
                config.output_budget_bytes = Some(1);
            }
            let cancel = Cancel::new();
            let watcher = if stop == "cancelled" {
                let executed = executed.clone();
                let cancel = cancel.clone();
                Some(std::thread::spawn(move || {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                    while !executed.exists() && std::time::Instant::now() < deadline {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    cancel.now();
                }))
            } else {
                None
            };
            let client = Client::over(replay.clone(), SecretSet::default());
            let mut launch = FirstRun::resolve(&homes.environment, Config::default())
                .unwrap()
                .answered_by(
                    &homes.machine(),
                    &config,
                    Enforcement::Unsafe,
                    FrontEnd::Terminal,
                    None,
                    |_, ask, tools| Asking::new(client.clone(), ask.route(route), tools),
                )
                .ok()
                .unwrap();
            let mut retaining = Retaining::default();
            let prompt = format!("Inspect {}", homes.root.path().display());
            let first = launch
                .drive(&prompt, &[], &mut Quiet, &mut retaining, cancel.clone())
                .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
            if let Some(watcher) = watcher {
                watcher.join().unwrap();
            }
            assert_eq!(
                first.stopped().word(),
                if stop == "failed" { "max_turns" } else { stop }
            );
            let earlier = retaining.0.turns().to_vec();
            launch
                .drive(
                    "Recover without repeating work",
                    &earlier,
                    &mut Quiet,
                    &mut retaining,
                    Cancel::new(),
                )
                .unwrap_or_else(|refusal| panic!("{}", refusal.message()));
            let requests = replay.requests();
            let body = &requests.last().unwrap().body;
            assert!(body.contains("PARTIAL_EVIDENCE"), "{stop}: {body}");
            assert_eq!(body.matches("call_done").count(), 2, "{stop}: {body}");
            assert!(body.contains("call_pending"), "{stop}: {body}");
            if stop == "budget" || stop == "cancelled" {
                assert!(body.contains("not executed"), "{body}");
                assert_eq!(body.matches("call_pending").count(), 2);
            }
            if stop == "error" {
                assert!(body.contains("command"));
                assert!(body.contains("failed"), "{body}");
            }
            let serialized: serde_json::Value = serde_json::from_str(body).unwrap();
            let field = if route == Route::Responses {
                "input"
            } else {
                "messages"
            };
            let messages = serialized[field].as_array().unwrap();
            if wire == Wire::Anthropic {
                let tool_results = messages
                    .iter()
                    .filter_map(|message| message["content"].as_array())
                    .flat_map(|blocks| blocks.iter())
                    .filter(|block| block["type"] == "tool_result")
                    .collect::<Vec<_>>();
                assert_eq!(tool_results.len(), 2);
                assert_eq!(tool_results[0]["tool_use_id"], "call_done");
                assert_eq!(tool_results[1]["tool_use_id"], "call_pending");
                assert_eq!(
                    tool_results[0]["is_error"].as_bool().unwrap_or(false),
                    stop == "failed",
                    "{stop}: {body}"
                );
                assert_eq!(
                    tool_results[1]["is_error"].as_bool().unwrap_or(false),
                    stop == "error" || stop == "budget" || stop == "cancelled"
                );
            }
            assert!(body.find("call_done").unwrap() < body.find("call_pending").unwrap());
            assert_eq!(body.matches("Recover without repeating work").count(), 1);
            assert_eq!(
                std::fs::read_to_string(&executed).unwrap(),
                if stop == "error" || stop == "budget" || stop == "cancelled" {
                    "x"
                } else {
                    "xy"
                }
            );
        }
    }
}

#[test]
fn known_capacity_refuses_large_retained_material_before_the_wire() {
    use sindri_core::asking::Asking;
    use sindri_core::provider::Client;
    use sindri_core::provider::transport::Replay;

    let homes = Homes::new();
    let replay = Replay::new([]);
    let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
    config.context_window = Some(20_000);
    let client = Client::over(replay.clone(), SecretSet::default());
    let mut launch = FirstRun::resolve(&homes.environment, Config::default())
        .unwrap()
        .answered_by(
            &homes.machine(),
            &config,
            Enforcement::Unsafe,
            FrontEnd::Terminal,
            None,
            |_, ask, tools| Asking::new(client.clone(), ask, tools),
        )
        .ok()
        .unwrap();
    let earlier = vec![Turn::answered(
        1,
        "Remember exact evidence",
        "x".repeat(30_000),
    )];
    let outcome = launch
        .drive(
            "Continue",
            &earlier,
            &mut Quiet,
            &mut Kept::default(),
            Cancel::new(),
        )
        .unwrap_or_else(|r| panic!("{}", r.message()));
    assert!(format!("{:?}", outcome).contains("capacity"));
    assert!(replay.requests().is_empty());
    assert_eq!(earlier[0].text.len(), 30_000);
}

#[test]
fn capacity_admission_preserves_full_replay_and_unknown_rejections() {
    use sindri_core::asking::Asking;
    use sindri_core::provider::Client;
    use sindri_core::provider::transport::{Canned, Replay};

    for capacity in [None, Some(200_000), Some(1)] {
        let homes = Homes::new();
        let replay = Replay::new([Canned::reply(
            400,
            r#"{"error":{"type":"invalid_request_error","message":"context window exceeded"}}"#,
        )]);
        let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
        config.context_window = capacity;
        let client = Client::over(replay.clone(), SecretSet::default());
        let mut launch = FirstRun::resolve(&homes.environment, Config::default())
            .unwrap()
            .answered_by(
                &homes.machine(),
                &config,
                Enforcement::Unsafe,
                FrontEnd::Terminal,
                None,
                |_, ask, tools| Asking::new(client.clone(), ask, tools),
            )
            .ok()
            .unwrap();
        let earlier = vec![Turn::answered(1, "EXACT_PROMPT", "EXACT_EVIDENCE")];
        let outcome = launch
            .drive(
                "Continue",
                &earlier,
                &mut Quiet,
                &mut Kept::default(),
                Cancel::new(),
            )
            .unwrap_or_else(|r| panic!("{}", r.message()));
        let requests = replay.requests();
        if capacity == Some(1) {
            assert!(
                requests.is_empty(),
                "instructions and tools alone exceed the known room"
            );
            assert!(format!("{outcome:?}").contains("capacity"));
        } else {
            assert_eq!(requests.len(), 1);
            assert!(requests[0].body.contains("EXACT_PROMPT"));
            assert!(requests[0].body.contains("EXACT_EVIDENCE"));
            assert!(requests[0].body.contains("bash"));
            assert!(format!("{outcome:?}").contains("context window exceeded"));
        }
        assert_eq!(earlier[0].text, "EXACT_EVIDENCE");
    }
}

#[test]
fn capacity_headroom_settings_reject_zero_negative_and_overflow() {
    for key in ["output_headroom", "tool_result_headroom"] {
        for value in ["0", "-1", "18446744073709551616"] {
            assert!(Config::field(key).unwrap().read(value).is_err());
        }
    }
}

#[test]
fn target_switch_keeps_evidence_without_transferring_opaque_reasoning() {
    use sindri_core::asking::Asking;
    use sindri_core::provider::transport::{Canned, Replay};
    use sindri_core::provider::{Client, Route};
    #[derive(Default)]
    struct Retaining(Vec<Turn>, usize);
    impl Turns for Retaining {
        fn turn(&mut self, _: &Turn) {}
        fn replay_boundary(&mut self) {
            self.1 += 1;
        }
        fn retained(&mut self, turns: &[Turn]) {
            self.0.extend_from_slice(turns);
        }
    }
    for change in ["endpoint", "model", "route"] {
        let homes = Homes::new();
        let fixture = include_str!("fixtures/providers/openai_responses_stream.sse").replace(
            r#""id":"rs_01","type":"reasoning","summary""#,
            r#""id":"rs_01","type":"reasoning","encrypted_content":"SWITCH_OPAQUE","summary""#,
        );
        let replay = Replay::new([
            Canned::stream(Box::leak(fixture.into_boxed_str())),
            Canned::stream(include_str!(
                "fixtures/providers/openai_responses_stream.sse"
            )),
            Canned::stream(if change == "route" {
                include_str!("fixtures/providers/copilot_chat_stream.sse")
            } else {
                include_str!("fixtures/providers/openai_responses_stream.sse")
            }),
        ]);
        let client = Client::over(replay.clone(), SecretSet::default());
        let mut retained = Retaining::default();
        for _ in 0..3 {
            let model = "gpt-5";
            let mut config = naming_wire(String::from(UNANSWERED_ENDPOINT));
            config.provider = Some(Wire::Openai);
            config.model = Some(model.into());
            let switching = retained.0.len() == 2;
            if switching && change == "endpoint" {
                config.base_url = Some("http://changed.invalid".into());
            }
            if switching && change == "model" {
                config.model = Some("o3".into());
            }
            let route = if switching && change == "route" {
                Route::Chat
            } else {
                Route::Responses
            };
            let mut launch = FirstRun::resolve(&homes.environment, Config::default())
                .unwrap()
                .answered_by(
                    &homes.machine(),
                    &config,
                    Enforcement::Unsafe,
                    FrontEnd::Terminal,
                    None,
                    |_, ask, tools| Asking::new(client.clone(), ask.route(route), tools),
                )
                .ok()
                .unwrap();
            let earlier = retained.0.clone();
            launch
                .drive(
                    "Keep the exact discovery",
                    &earlier,
                    &mut Quiet,
                    &mut retained,
                    Cancel::new(),
                )
                .unwrap_or_else(|r| panic!("{}", r.message()));
        }
        let requests = replay.requests();
        assert_eq!(requests.len(), 3);
        assert_eq!(retained.1, 1);
        assert!(requests[1].body.contains("SWITCH_OPAQUE"));
        assert!(!requests[2].body.contains("SWITCH_OPAQUE"));
        assert!(requests[2].body.contains("Hi there"));
        let body: serde_json::Value = serde_json::from_str(&requests[2].body).unwrap();
        assert!(body.get("store").is_none());
        assert!(body.get("previous_response_id").is_none());
    }
}
