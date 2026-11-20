//! What a launch is, proved at the core's own seam: one value that joins the boundary, the proof,
//! the session, the record, the tools and the client in the one order a run can be asked for.

use std::collections::BTreeMap;
use std::fs::Permissions;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sindri_core::config::Config;
use sindri_core::launch::{Handed, Launch, Machine, Turns};
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::provider::Deltas;
use sindri_core::records::{Entry, SessionDocument, read_transcript};
use sindri_core::run::{Cancel, Stop, Turn};
use sindri_core::tools::FrontEnd;
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

/// A wire nothing answers on: a loopback port that was bound and let go, so a run that reaches it is
/// refused at once instead of waiting out a timeout.
fn closed_wire() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

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
    let wire = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    wire.set_nonblocking(true)
        .expect("a wire that never blocks");
    let port = wire.local_addr().expect("a bound address").port();

    let Err(refusal) = Launch::open(
        &machine,
        &naming_wire(format!("http://127.0.0.1:{port}")),
        Enforcement::Bubblewrap,
        FrontEnd::Batch,
        None,
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
    let asked = wire
        .accept()
        .map(|_| ())
        .expect_err("a launch refused by the sandbox asks for no turn");
    assert_eq!(
        asked.kind(),
        ErrorKind::WouldBlock,
        "the wire was never asked: {asked}"
    );
}

/// A front end that writes no fragment: batch mode's answer is written once it is whole.
struct Quiet;

impl Deltas for Quiet {
    fn text(&mut self, _fragment: &str) {}

    fn reasoning(&mut self, _fragment: &str) {}
}

/// A front end that keeps the newest document and transcript it is handed, which is what a launch's
/// own record says once a run has stopped.
#[derive(Default)]
struct Kept {
    document: Option<SessionDocument>,
    transcript: Vec<Entry>,
}

impl Turns for Kept {
    fn turn(&mut self, _ended: &Turn) {}

    fn facts(&mut self, handed: Handed<'_>) {
        self.document = Some(handed.document.clone());
        self.transcript = handed.transcript.to_vec();
    }
}

#[test]
fn a_launch_records_the_wire_the_model_and_the_boundary_it_runs() {
    let homes = Homes::new();

    let mut launch = Launch::open(
        &homes.machine(),
        &naming_wire(closed_wire()),
        Enforcement::Unsafe,
        FrontEnd::Batch,
        None,
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");
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
    let outcome = match launch.drive("triage this", &[], &mut Quiet, &mut kept, Cancel::new()) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    };

    assert!(
        matches!(outcome.stopped(), Stop::Error(_)),
        "a closed wire ends the run rather than answering it: {}",
        outcome.stopped()
    );
    let document = kept
        .document
        .expect("the launch hands the document its record holds");
    assert_eq!(document.wire, Wire::Anthropic);
    assert_eq!(document.model, "claude-sonnet-4-5");
    assert_eq!(document.enforcement, Enforcement::Unsafe);
    assert_eq!(
        document.turns, 0,
        "a wire that took no reply ended the run before a turn was kept"
    );
    let transcript = read_transcript(&figure.session).expect("the record the launch wrote");
    let (Some(Entry::Prompt { .. }), Some(Entry::Stop { .. })) =
        (transcript.entries().first(), transcript.entries().last())
    else {
        panic!(
            "the record opens with the prompt it was asked and closes with the stop: {:?}",
            transcript.entries()
        );
    };
    assert!(
        figure.session.join("session.json").exists(),
        "the record the launch wrote is in the session it named"
    );
}

#[test]
fn a_launch_hands_the_transcript_its_record_wrote() {
    let homes = Homes::new();
    let mut launch = Launch::open(
        &homes.machine(),
        &naming_wire(closed_wire()),
        Enforcement::Unsafe,
        FrontEnd::Terminal,
        None,
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");
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
        &naming_wire(closed_wire()),
        Enforcement::Unsafe,
        FrontEnd::Terminal,
        None,
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");
    let second = Launch::open(
        &homes.machine(),
        &naming_wire(closed_wire()),
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
    let mut launch = Launch::open(
        &homes.machine(),
        &naming_wire(closed_wire()),
        Enforcement::Unsafe,
        FrontEnd::Terminal,
        None,
    )
    .ok()
    .expect("a config naming a wire, a model and a key names a run");
    let session = launch.session();

    let first = match launch.drive("the first prompt", &[], &mut Quiet, &mut (), Cancel::new()) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run stopped on its own: {}", refusal.message()),
    };
    let second = match launch.drive("the second prompt", &[], &mut Quiet, &mut (), Cancel::new()) {
        Ok(outcome) => outcome,
        Err(refusal) => panic!("the run after it stopped on its own: {}", refusal.message()),
    };

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
    let config = naming_wire(closed_wire());

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
        &naming_wire(closed_wire()),
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
        &naming_wire(closed_wire()),
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
