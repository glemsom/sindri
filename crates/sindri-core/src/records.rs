use crate::policy::{Boundary, Enforcement, PolicyRecord};
use crate::provider::Route;
use crate::provider::Usage as Ask;
use crate::redact::{Redacted, SecretSet};
use crate::run::{Acted, Call, Exit, Limits, State, Stop};
use crate::session::{Session, SessionError};
use crate::wire::Wire;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
const SESSION: &str = "session.json";
const STAGING: &str = "session.json.tmp";
const TRANSCRIPT: &str = "transcript.jsonl";
const STATUS: &str = "status.json";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    Turn {
        turn: u32,
    },
    Call {
        turn: u32,
        name: Redacted,
        arguments: Redacted,
    },
    Result {
        turn: u32,
        name: Redacted,
        text: Redacted,
        bytes: u64,
        elapsed_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit: Option<Exit>,
    },
    Reasoning {
        turn: u32,
        text: Redacted,
    },
    Assistant {
        turn: u32,
        text: Redacted,
    },
    Prompt {
        text: Redacted,
    },
    Stop {
        stop: Stop,
    },
}
pub struct Facts<'a> {
    pub wire: Wire,
    pub model: &'a str,
    pub route: Option<Route>,
    pub thinking: bool,
    pub boundary: &'a Boundary,
    pub limits: &'a Limits,
    pub workspace: &'a Path,
    pub secrets: &'a SecretSet,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Dials {
    pub max_turns: u32,
    pub command_budget_ms: u64,
    pub output_budget_bytes: u64,
}
impl Dials {
    pub fn of(limits: &Limits) -> Dials {
        Dials {
            max_turns: limits.max_turns(),
            command_budget_ms: millis(limits.clock()),
            output_budget_bytes: limits.output(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u64>,
}
impl Usage {
    pub fn charge(&mut self, ask: &Ask) {
        self.input = added(self.input, ask.input);
        self.output = added(self.output, ask.output);
        self.total = added(self.total, ask.total);
        self.cached_input = added(self.cached_input, ask.cached_input);
        self.reasoning = added(self.reasoning, ask.reasoning);
    }
}
fn added(total: Option<u64>, ask: Option<u64>) -> Option<u64> {
    match (total, ask) {
        (Some(total), Some(ask)) => Some(total.saturating_add(ask)),
        (Some(total), None) => Some(total),
        (None, ask) => ask,
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionDocument {
    pub session: String,
    #[serde(rename = "provider")]
    pub wire: Wire,
    pub model: String,
    pub enforcement: Enforcement,
    pub policy: PolicyRecord,
    pub workspace: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub dials: Dials,
    pub start: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub turns: u32,
    pub usage: Usage,
    pub stopped: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<Route>,
    pub thinking: bool,
}
pub struct Record {
    session: Session,
    document: SessionDocument,
    secrets: SecretSet,
    transcript: File,
    statuses: File,
}
impl Record {
    pub fn open(session: &Session, facts: Facts<'_>) -> Result<Record, RecordError> {
        let checkout = checkout(facts.workspace);
        let document = SessionDocument {
            session: session.guid().to_owned(),
            wire: facts.wire,
            model: facts.model.to_owned(),
            enforcement: facts.boundary.enforcement(),
            policy: facts.boundary.record_policy(),
            workspace: facts.workspace.to_path_buf(),
            branch: checkout.branch,
            commit: checkout.commit,
            dials: Dials::of(facts.limits),
            start: timestamp(SystemTime::now()),
            end: None,
            turns: 0,
            usage: Usage::default(),
            stopped: State::Running,
            route: facts.route,
            thinking: facts.thinking,
        };
        let transcript = appending(&session.directory().join(TRANSCRIPT))?;
        let statuses = appending(&session.directory().join(STATUS))?;
        let mut record = Record {
            session: session.clone(),
            document,
            secrets: facts.secrets.clone(),
            transcript,
            statuses,
        };
        record.write_document()?;
        Ok(record)
    }
    pub fn guid(&self) -> &str {
        self.session.guid()
    }
    pub fn document(&self) -> &SessionDocument {
        &self.document
    }
    /// A run joins the session this record keeps: the session is in flight again, and the end the
    /// run before it reached is not this one's. The turns it already counted stay counted.
    pub fn began(&mut self) -> Result<(), RecordError> {
        self.document.stopped = State::Running;
        self.document.end = None;
        self.write_document()
    }
    /// The session's turn, which is what the document counts and what the transcript is numbered by:
    /// a turn a later prompt takes is numbered after the turns every prompt before it ended.
    pub fn turn_started(&mut self, turn: u32) -> Result<(), RecordError> {
        self.append(Entry::Turn { turn })?;
        self.document.turns = turn;
        self.write_document()
    }
    pub fn asked(&mut self, text: &str) -> Result<(), RecordError> {
        self.append(Entry::Prompt {
            text: self.secrets.redact(text),
        })
    }
    pub fn called(&mut self, turn: u32, call: &Call) -> Result<(), RecordError> {
        self.append(Entry::Call {
            turn,
            name: self.secrets.redact(call.name.clone()),
            arguments: self.secrets.redact(call.arguments.clone()),
        })
    }
    pub fn resulted(&mut self, turn: u32, call: &Call, acted: &Acted) -> Result<(), RecordError> {
        self.append(Entry::Result {
            turn,
            name: self.secrets.redact(call.name.clone()),
            text: self.secrets.redact(acted.text.clone()),
            bytes: acted.bytes,
            elapsed_ms: millis(acted.elapsed),
            exit: Some(acted.exit),
        })
    }
    pub fn reasoned(&mut self, turn: u32, text: &str) -> Result<(), RecordError> {
        self.append(Entry::Reasoning {
            turn,
            text: self.secrets.redact(text),
        })
    }
    pub fn answered(&mut self, turn: u32, text: &str) -> Result<(), RecordError> {
        self.append(Entry::Assistant {
            turn,
            text: self.secrets.redact(text),
        })
    }
    pub fn stopped(&mut self, state: &State) -> Result<(), RecordError> {
        self.document.stopped = state.clone();
        if !state.is_running() {
            self.document.end = Some(timestamp(SystemTime::now()));
        }
        self.write_document()?;
        let Some(stop) = state.stop() else {
            return Ok(());
        };
        let stop = match stop {
            Stop::Error(message) => Stop::Error(self.secrets.redact(message.to_string())),
            stop => stop.clone(),
        };
        self.append(Entry::Stop { stop })
    }
    pub fn spent(&mut self, usage: Usage) -> Result<(), RecordError> {
        self.document.usage = usage;
        self.write_document()
    }
    pub fn charged(&mut self, ask: &Ask) -> Result<(), RecordError> {
        self.document.usage.charge(ask);
        self.write_document()
    }
    pub fn told(&mut self, told: Told) -> Result<(), RecordError> {
        let status = Status::new(told, &self.document.workspace);
        let path = self.session.directory().join(STATUS);
        write_line(&mut self.statuses, &path, &status)
    }
    fn append(&mut self, entry: Entry) -> Result<(), RecordError> {
        let path = self.session.directory().join(TRANSCRIPT);
        write_line(&mut self.transcript, &path, &entry)
    }
    pub fn finish(&mut self) -> Result<(), SessionError> {
        self.session.finish()
    }
    fn write_document(&mut self) -> Result<(), RecordError> {
        let directory = self.session.directory();
        let staging = directory.join(STAGING);
        let mut bytes = serde_json::to_vec_pretty(&self.document).map_err(|error| {
            RecordError::Unserializable {
                reason: error.to_string(),
            }
        })?;
        bytes.push(b'\n');
        std::fs::write(&staging, &bytes).map_err(|error| RecordError::Unwritable {
            path: staging.clone(),
            reason: error.to_string(),
        })?;
        std::fs::rename(&staging, directory.join(SESSION)).map_err(|error| {
            RecordError::Unwritable {
                path: directory.join(SESSION),
                reason: error.to_string(),
            }
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Checkout {
    branch: Option<String>,
    commit: Option<String>,
}
fn checkout(workspace: &Path) -> Checkout {
    Checkout {
        branch: branch(workspace),
        commit: git(workspace, &["rev-parse", "HEAD"]),
    }
}

/// The branch `workspace` stands on, for a launch that has no session yet, and `None` where a
/// checkout stands on none: no repository, a detached head that names itself `HEAD`, or no git.
pub fn branch(workspace: &Path) -> Option<String> {
    git(workspace, &["rev-parse", "--abbrev-ref", "HEAD"]).filter(|name| name != "HEAD")
}
fn git(workspace: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let said = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!said.is_empty()).then_some(said)
}
pub fn timestamp(time: SystemTime) -> String {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        Err(error) => {
            let before = error.duration();
            let whole = -i64::try_from(before.as_secs()).unwrap_or(i64::MAX);
            match before.subsec_nanos() {
                0 => whole,
                _ => whole.saturating_sub(1),
            }
        }
    };
    let (year, month, day) = calendar(seconds.div_euclid(86_400));
    let second_of_day = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (
        second_of_day / 3_600,
        (second_of_day / 60) % 60,
        second_of_day % 60,
    );
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}
fn calendar(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month + 2) / 5 + 1;
    let month = if month < 10 { month + 3 } else { month - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
#[derive(Debug)]
pub enum RecordError {
    Unwritable { path: PathBuf, reason: String },
    Unserializable { reason: String },
    Unreadable { path: PathBuf, reason: String },
}
impl fmt::Display for RecordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordError::Unwritable { path, reason } => {
                write!(formatter, "cannot write {}: {reason}", path.display())
            }
            RecordError::Unserializable { reason } => {
                write!(formatter, "cannot write a record as JSON: {reason}")
            }
            RecordError::Unreadable { path, reason } => {
                write!(formatter, "cannot read {}: {reason}", path.display())
            }
        }
    }
}
impl Error for RecordError {}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}
impl Phase {
    pub fn word(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Working => "working",
            Phase::Done => "done",
            Phase::Blocked => "blocked",
            Phase::Error => "error",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Told {
    Idle,
    Asking { turn: u32 },
    Waiting { turn: u32 },
    Stopped { turn: u32, stop: Stop },
}
impl Told {
    pub fn phase(&self) -> Phase {
        match self {
            Told::Idle => Phase::Idle,
            Told::Asking { .. } => Phase::Working,
            Told::Waiting { .. } => Phase::Blocked,
            Told::Stopped { stop, .. } => match stop {
                Stop::Error(_) => Phase::Error,
                Stop::Final | Stop::MaxTurns | Stop::Budget(_) | Stop::Cancelled => Phase::Done,
            },
        }
    }
    pub fn line(&self) -> &'static str {
        match self {
            Told::Idle => "waiting for a prompt",
            Told::Asking { .. } => "asking the model",
            Told::Waiting { .. } => "waiting for a command",
            Told::Stopped { stop, .. } => match stop {
                Stop::Final => "answered",
                Stop::MaxTurns => "reached the turn cap",
                Stop::Budget(_) => "spent a budget",
                Stop::Cancelled => "cancelled",
                Stop::Error(_) => "failed",
            },
        }
    }
    pub fn turn(&self) -> u32 {
        match self {
            Told::Idle => 0,
            Told::Asking { turn } | Told::Waiting { turn } | Told::Stopped { turn, .. } => *turn,
        }
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Status {
    phase: Phase,
    message: Redacted,
    turn: u32,
    workspace: String,
}
impl Status {
    pub fn new(told: Told, workspace: &Path) -> Status {
        Status {
            phase: told.phase(),
            message: SecretSet::default().redact(told.line()),
            turn: told.turn(),
            workspace: name_of(workspace),
        }
    }
    pub fn json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a status is four facts and always serialises")
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn message(&self) -> &Redacted {
        &self.message
    }
    pub fn turn(&self) -> u32 {
        self.turn
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
}
fn name_of(workspace: &Path) -> String {
    match workspace.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => workspace.display().to_string(),
    }
}
fn write_line(file: &mut File, path: &Path, value: &impl Serialize) -> Result<(), RecordError> {
    let mut line = serde_json::to_vec(value).map_err(|error| RecordError::Unserializable {
        reason: error.to_string(),
    })?;
    line.push(b'\n');
    file.write_all(&line)
        .map_err(|error| RecordError::Unwritable {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })
}
fn appending(path: &Path) -> Result<File, RecordError> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| RecordError::Unwritable {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transcript {
    entries: Vec<Entry>,
    skipped: usize,
}
impl Transcript {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn skipped(&self) -> usize {
        self.skipped
    }
}
pub fn read_lines(path: &Path) -> Result<Vec<String>, RecordError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(RecordError::Unreadable {
                path: path.to_path_buf(),
                reason: error.to_string(),
            });
        }
    };
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| serde_json::from_str::<serde_json::Value>(line).is_ok())
        .map(str::to_owned)
        .collect())
}
pub fn read_status(directory: &Path) -> Result<Vec<Status>, RecordError> {
    let path = status_path(directory);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(RecordError::Unreadable {
                path,
                reason: error.to_string(),
            });
        }
    };
    Ok(text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Status>(line).ok())
        .collect())
}
pub fn read_transcript(directory: &Path) -> Result<Transcript, RecordError> {
    let path = transcript_path(directory);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Transcript::default());
        }
        Err(error) => {
            return Err(RecordError::Unreadable {
                path,
                reason: error.to_string(),
            });
        }
    };
    Ok(Transcript::read(&text))
}
pub fn document_path(directory: &Path) -> PathBuf {
    directory.join(SESSION)
}
pub fn transcript_path(directory: &Path) -> PathBuf {
    directory.join(TRANSCRIPT)
}
pub fn status_path(directory: &Path) -> PathBuf {
    directory.join(STATUS)
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Recorded {
    #[serde(skip)]
    directory: PathBuf,
    session: String,
    #[serde(rename = "provider")]
    wire: Wire,
    model: String,
    enforcement: Enforcement,
    workspace: PathBuf,
    branch: Option<String>,
    commit: Option<String>,
    start: String,
    end: Option<String>,
    turns: u32,
    usage: Usage,
    stopped: String,
}
impl Recorded {
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn guid(&self) -> &str {
        &self.session
    }
    pub fn wire(&self) -> Wire {
        self.wire
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn enforcement(&self) -> Enforcement {
        self.enforcement
    }
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }
    pub fn commit(&self) -> Option<&str> {
        self.commit.as_deref()
    }
    pub fn start(&self) -> &str {
        &self.start
    }
    pub fn end(&self) -> Option<&str> {
        self.end.as_deref()
    }
    pub fn turns(&self) -> u32 {
        self.turns
    }
    pub fn usage(&self) -> Usage {
        self.usage
    }
    pub fn stopped(&self) -> &str {
        &self.stopped
    }
}
pub fn read_sessions(root: &Path) -> Result<Vec<Recorded>, RecordError> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(RecordError::Unreadable {
                path: root.to_path_buf(),
                reason: error.to_string(),
            });
        }
    };
    let mut sessions = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| RecordError::Unreadable {
            path: root.to_path_buf(),
            reason: error.to_string(),
        })?;
        let directory = entry.path();
        if let Some(session) = read_session(&directory)? {
            sessions.push(session);
        }
    }
    sessions.sort_by(|left, right| {
        right
            .start
            .cmp(&left.start)
            .then_with(|| left.session.cmp(&right.session))
    });
    Ok(sessions)
}
pub fn choose<'a>(sessions: &'a [Recorded], id: &str) -> Result<&'a Recorded, LookupError> {
    let matching: Vec<&Recorded> = sessions
        .iter()
        .filter(|session| session.guid().starts_with(id))
        .collect();
    match matching.as_slice() {
        [one] => Ok(one),
        [] => Err(LookupError::Absent { id: id.to_owned() }),
        many => Err(LookupError::Ambiguous {
            id: id.to_owned(),
            candidates: many
                .iter()
                .map(|session| session.guid().to_owned())
                .collect(),
        }),
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupError {
    Ambiguous { id: String, candidates: Vec<String> },
    Absent { id: String },
}
impl LookupError {
    pub fn exit_code(&self) -> u8 {
        2
    }
    pub fn candidates(&self) -> &[String] {
        match self {
            LookupError::Ambiguous { candidates, .. } => candidates,
            LookupError::Absent { .. } => &[],
        }
    }
}
impl fmt::Display for LookupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LookupError::Ambiguous { id, candidates } => write!(
                formatter,
                "`{id}` names more than one session: {}",
                candidates.join(", ")
            ),
            LookupError::Absent { id } => write!(formatter, "no session matches `{id}`"),
        }
    }
}
impl Error for LookupError {}
fn read_session(directory: &Path) -> Result<Option<Recorded>, RecordError> {
    let path = document_path(directory);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(RecordError::Unreadable {
                path,
                reason: error.to_string(),
            });
        }
    };
    Ok(serde_json::from_str::<Recorded>(&text)
        .ok()
        .map(|mut session| {
            session.directory = directory.to_path_buf();
            session
        }))
}
impl Transcript {
    fn read(text: &str) -> Transcript {
        let mut transcript = Transcript::default();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            match serde_json::from_str::<Entry>(line) {
                Ok(entry) => transcript.entries.push(entry),
                Err(_) => transcript.skipped += 1,
            }
        }
        transcript
    }
}
