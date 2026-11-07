//! Records are the session directory, and the entries written into it as the run
//! goes: `session.json` says what the run was and how it ended,
//! `transcript.jsonl` holds one entry per thing that happened, and `status.json`
//! holds the status documents the run said about itself.
//!
//! Everything here is written when it happens and never at the end, because a
//! cancelled or killed run still has to read: a document is rewritten through a
//! staging file and a rename, so a reader never sees half of one, and a line of
//! the transcript is one `write` to an append-only file, so the kernel holds an
//! entry before the next is made. Nothing is buffered for the whole run.
//!
//! An [`Entry`] is a fact and never a rendering (ADR-0007): the TUI and a later
//! reader draw the same run from the same values, and no entry carries a
//! formatted line. Durability is turn granular — a turn starts, a call is made, a
//! result lands, the assistant answers — so a `kill -9` costs the work in flight
//! and not the session.
//!
//! Reading is deliberately more tolerant than writing is: a line this build
//! cannot read is skipped and the rest of the transcript still loads, because a
//! newer `sindri` may always write what an older one reads past.
//!
//! Every string that reaches these files goes through the run's [`SecretSet`],
//! so the record is a window that cannot carry a secret even when a command
//! prints one.

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::policy::{Boundary, Enforcement, PolicyRecord};
use crate::provider::Route;
use crate::provider::Usage as Ask;
use crate::redact::{Redacted, SecretSet};
use crate::run::{Acted, Call, Exit, Limits, State};
use crate::session::{Session, SessionError};
use crate::wire::Wire;

/// The session document: the run as it stands, rewritten as facts land.
const SESSION: &str = "session.json";

/// The document while it is being replaced, so a reader sees the old one or the
/// new one and never a half-written file.
const STAGING: &str = "session.json.tmp";

/// One entry per thing that happened, in order, one line each.
const TRANSCRIPT: &str = "transcript.jsonl";

/// The status documents the run said about itself, one per line.
const STATUS: &str = "status.json";

/// One thing that happened in a run, in order, as the transcript writes it.
///
/// The tag is `kind`, and the variants are the moments durability is granular to
/// (ADR-0007): a turn starts, a call is made, a result lands, the turn reasons, and
/// the assistant answers when the turn ends. Every field is a fact the run was
/// handed — the model's own words, the wire's own arguments — and never a
/// formatted line, so the interface and any reader draw one run from one value.
///
/// Forward compatibility is these two rules and nothing else: [`read_transcript`]
/// skips a `kind` this build does not know, and a field it does not know is
/// ignored, so a newer `sindri` may always write what an older one reads past.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// A turn started: the run asked the model for one more.
    Turn {
        /// Which turn, counted from one.
        turn: u32,
    },
    /// The model asked for a tool call.
    Call {
        /// Which turn asked for it.
        turn: u32,
        /// The tool, as the schema the model was given names it.
        name: String,
        /// The arguments, as the wire wrote them.
        arguments: Redacted,
    },
    /// A call came back with what it did.
    Result {
        /// Which turn asked for it.
        turn: u32,
        /// The tool that ran.
        name: String,
        /// What the model is told about it.
        text: Redacted,
        /// The command output bytes it cost the run.
        bytes: u64,
        /// How long it held the run's clock.
        elapsed_ms: u64,
        /// How the call ended, which is what its row says — and nothing at all on a
        /// line a build wrote before the verdict was a fact, which still reads
        /// (ADR-0007) and is left out of the file rather than written as `null`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit: Option<Exit>,
    },
    /// The turn reasoned before it answered: what the model said to itself, settled
    /// as the one fact it is.
    ///
    /// A thought has no partial form here: fragments land in the front end's own
    /// value and never in the transcript (ADR-0007), so this is the whole of what one
    /// turn reasoned, written when that turn stopped reasoning. A build that knows no
    /// `reasoning` skips the line, which is the forward rule this enum keeps.
    Reasoning {
        /// Which turn reasoned.
        turn: u32,
        /// What the model said to itself, as it settled.
        text: Redacted,
    },
    /// The turn ended with what the assistant said.
    Assistant {
        /// Which turn ended.
        turn: u32,
        /// What the model answered with.
        text: Redacted,
    },
}

/// The facts a session document opens with: what the run is, before the first
/// thing has happened.
///
/// Every field is an input and not a reading, so the document cannot say
/// something about the run that the run was not handed, and a test opens a
/// record without touching a real state home, clock or boundary.
pub struct Facts<'a> {
    /// The provider dialect the run asks.
    pub wire: Wire,
    /// The model it asks for, named rather than invented.
    pub model: &'a str,
    /// The completion endpoint the wire answered on, for a wire that serves two
    /// of them (Appendix A10), and nothing for one that serves a single shape.
    pub route: Option<Route>,
    /// The boundary the model was told about and every command is held to.
    pub boundary: &'a Boundary,
    /// The dials the run obeys.
    pub limits: &'a Limits,
    /// The one directory the run stands in.
    pub workspace: &'a Path,
    /// Every literal the record must never write down.
    pub secrets: &'a SecretSet,
}

/// The dials a run obeys, as the document keeps them: the turn cap and the two
/// run-scoped budgets, read off the [`Limits`] in force so the record cannot
/// name a dial the loop was not given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Dials {
    /// The ceiling on turns.
    pub max_turns: u32,
    /// The run's command wall clock, in milliseconds.
    pub command_budget_ms: u64,
    /// The run's command output, in bytes.
    pub output_budget_bytes: u64,
}

impl Dials {
    /// The dials `limits` holds.
    pub fn of(limits: &Limits) -> Dials {
        Dials {
            max_turns: limits.max_turns(),
            command_budget_ms: millis(limits.clock()),
            output_budget_bytes: limits.output(),
        }
    }
}

/// What a run's asks spent, summed over every turn: the tokens the wire
/// reported, and nothing at all for a token count nobody reported, because a
/// field the run knows nothing about is left out rather than guessed at (§15).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(default)]
pub struct Usage {
    /// Every input token, cache reads included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    /// Every output token, reasoning included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    /// `input + output`, when both were reported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// The part of `input` the provider served from its own cache.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input: Option<u64>,
    /// The part of `output` spent reasoning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u64>,
}

impl Usage {
    /// Adds one ask's report to the run's total.
    pub fn charge(&mut self, ask: &Ask) {
        self.input = added(self.input, ask.input);
        self.output = added(self.output, ask.output);
        self.total = added(self.total, ask.total);
        self.cached_input = added(self.cached_input, ask.cached_input);
        self.reasoning = added(self.reasoning, ask.reasoning);
    }
}

/// Two token counts summed: a count nobody reported leaves the other alone, and
/// two nobodies are still nobody.
fn added(total: Option<u64>, ask: Option<u64>) -> Option<u64> {
    match (total, ask) {
        (Some(total), Some(ask)) => Some(total.saturating_add(ask)),
        (Some(total), None) => Some(total),
        (None, ask) => ask,
    }
}

/// The session document: provider, model, the route it answered on where the
/// wire serves two, enforcement, the policy handed to the model, the workspace,
/// the branch and commit it stood on, the dials obeyed, when it started and
/// ended, how many turns it started, what it spent, and why it stopped —
/// `running` until a stop reason lands (§13, Appendix A10).
///
/// Field order is fixed and later fields only append, and a field the run knows
/// nothing about is left out rather than written as `null` (§15), so the JSON of
/// two runs that did the same thing is the same JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SessionDocument {
    /// The GUID the session directory is named by.
    pub session: String,
    /// The provider dialect the run asked.
    #[serde(rename = "provider")]
    pub wire: Wire,
    /// The model it asked for.
    pub model: String,
    /// How its commands were confined.
    pub enforcement: Enforcement,
    /// The boundary the model was told about, verbatim.
    pub policy: PolicyRecord,
    /// The one directory the run stood in.
    pub workspace: PathBuf,
    /// The branch the workspace's checkout was on, when it was on one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The commit the workspace's checkout was at, when it was at one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The dials the run obeyed.
    pub dials: Dials,
    /// When the run started, as an RFC 3339 instant in UTC.
    pub start: String,
    /// When it stopped, in the same shape, once a stop reason landed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    /// How many turns it started.
    pub turns: u32,
    /// What its asks spent.
    pub usage: Usage,
    /// Why it stopped: `running` until a stop reason lands, then §4's word.
    pub stopped: State,
    /// The completion endpoint the wire answered on, when it serves more than
    /// one: `chat` or `responses`, so a run that answered oddly says where its
    /// reply came from (Appendix A10). Left out for a dialect that serves a
    /// single endpoint, because a field the run knows nothing about is never
    /// written (§15).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<Route>,
}

/// One session's records: the three files, the document held in memory and
/// rewritten as facts land, and the secrets every line is scrubbed through.
pub struct Record {
    /// The session whose directory the files live in.
    session: Session,
    /// The document as it stands, written whole on every change.
    document: SessionDocument,
    /// Every literal no line may carry.
    secrets: SecretSet,
    /// The transcript, opened for appending and written one entry at a time.
    transcript: File,
    /// The status documents, opened for appending in the same way.
    statuses: File,
}

impl Record {
    /// Opens the records for `session`, writing the session document in its
    /// `running` shape and naming the branch and commit the workspace stands on.
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

    /// The GUID this session is named by: the document's `session` field, and
    /// the last line `-v` prints.
    pub fn guid(&self) -> &str {
        self.session.guid()
    }

    /// The document as it stands.
    pub fn document(&self) -> &SessionDocument {
        &self.document
    }

    /// A turn started: written before the request goes out, so a run killed
    /// mid-turn says which turn it was in.
    pub fn turn_started(&mut self, turn: u32) -> Result<(), RecordError> {
        self.append(Entry::Turn { turn })?;
        self.document.turns = turn;
        self.write_document()
    }

    /// The model asked for `call`: written before the call runs, so a run killed
    /// inside a call says what it was doing.
    pub fn called(&mut self, turn: u32, call: &Call) -> Result<(), RecordError> {
        self.append(Entry::Call {
            turn,
            name: call.name.clone(),
            arguments: self.secrets.redact(call.arguments.clone()),
        })
    }

    /// `call` came back with `acted`: what the model is told, what the run paid for
    /// it, and how it ended.
    pub fn resulted(&mut self, turn: u32, call: &Call, acted: &Acted) -> Result<(), RecordError> {
        self.append(Entry::Result {
            turn,
            name: call.name.clone(),
            text: self.secrets.redact(acted.text.clone()),
            bytes: acted.bytes,
            elapsed_ms: millis(acted.elapsed),
            exit: Some(acted.exit),
        })
    }

    /// The turn reasoned `text` before it answered: written as its own fact beside the
    /// answer it reached, so a reader can show a run's thinking and a build that knows
    /// no `reasoning` reads past it (ADR-0007).
    pub fn reasoned(&mut self, turn: u32, text: &str) -> Result<(), RecordError> {
        self.append(Entry::Reasoning {
            turn,
            text: self.secrets.redact(text),
        })
    }

    /// The turn ended with what the assistant said.
    pub fn answered(&mut self, turn: u32, text: &str) -> Result<(), RecordError> {
        self.append(Entry::Assistant {
            turn,
            text: self.secrets.redact(text),
        })
    }

    /// A stop reason landed: the document says which, and when, and a run that
    /// is still running is not given an end it did not reach.
    pub fn stopped(&mut self, state: &State) -> Result<(), RecordError> {
        self.document.stopped = state.clone();
        if !state.is_running() {
            self.document.end = Some(timestamp(SystemTime::now()));
        }
        self.write_document()
    }

    /// The tokens the run's asks spent in total, written once when the run stops
    /// by whoever holds the sum: an ask's worth at a time would be the same
    /// arithmetic twice.
    pub fn spent(&mut self, usage: Usage) -> Result<(), RecordError> {
        self.document.usage = usage;
        self.write_document()
    }

    /// Charges one ask's tokens to the run, so the document's usage is what the
    /// run spent and not what its last ask spent.
    pub fn charged(&mut self, ask: &Ask) -> Result<(), RecordError> {
        self.document.usage.charge(ask);
        self.write_document()
    }

    /// What the run says it is doing (§14), written as one status document: the
    /// run's own words and never the command, the prompt or the answer.
    pub fn told(&mut self, phase: Phase, message: &str, turn: u32) -> Result<(), RecordError> {
        let status = Status {
            phase,
            message: self.secrets.redact(message),
            turn,
            workspace: name_of(&self.document.workspace),
        };
        let path = self.session.directory().join(STATUS);
        write_line(&mut self.statuses, &path, &status)
    }

    /// Writes one entry: the line is made whole and written in one call, so a
    /// kill lands between entries and never inside one.
    fn append(&mut self, entry: Entry) -> Result<(), RecordError> {
        let path = self.session.directory().join(TRANSCRIPT);
        write_line(&mut self.transcript, &path, &entry)
    }

    /// Ends the session: the temp is emptied and everything else is kept, so a
    /// session that ended the way it meant to is readable beside one that did
    /// not.
    pub fn finish(self) -> Result<(), SessionError> {
        let mut session = self.session;
        session.finish()
    }

    /// Replaces the session document: written to a staging file in the same
    /// directory and then renamed over the old one, which is the step that makes
    /// a reader see the whole of one document or the whole of the other.
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

/// Where the workspace's checkout stands, as git answers: the branch it is on and
/// the commit it is at, or nothing when the workspace is no checkout at all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Checkout {
    /// The branch, and nothing when `HEAD` is detached.
    branch: Option<String>,
    /// The commit `HEAD` names, whole.
    commit: Option<String>,
}

/// The checkout `workspace` stands on, asked of git where it is and read from
/// nowhere else: a workspace that is no repository says nothing rather than
/// guessing, and a tree that moved under the run cannot make the record lie
/// about the moment it was opened.
fn checkout(workspace: &Path) -> Checkout {
    Checkout {
        // A detached `HEAD` is a commit and not a branch, and `rev-parse` says
        // `HEAD` when it has no other name to give.
        branch: git(workspace, &["rev-parse", "--abbrev-ref", "HEAD"])
            .filter(|name| name != "HEAD"),
        commit: git(workspace, &["rev-parse", "HEAD"]),
    }
}

/// One git command asked in `workspace`, trimmed; nothing when git is absent,
/// refuses, or has nothing to say — an unborn branch has no commit, and a
/// directory that is no repository has neither.
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

/// `time` as the RFC 3339 instant in UTC that §13's start and end are read back
/// as, to the second: the shape a record is compared in, and one no reader has
/// to guess the timezone of.
pub fn timestamp(time: SystemTime) -> String {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        // Before the epoch, the instant is a whole number of seconds back and
        // then a part of one more, because a fraction of a second before the
        // epoch is a whole second earlier and never the epoch itself. The
        // calendar below counts backwards through a whole day at a time, which
        // is what `div_euclid` is for.
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
    // Positional, so the widths are all the string holds: a format that spelled
    // `{year:04}` and `{month:02}` side by side reads, to the tree's model-list
    // scan, as two served names written out by hand.
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

/// The civil date `days` after 1970-01-01, by the one arithmetic every calendar
/// conversion uses: shift the epoch to March so the leap day lands at the end of
/// a year, count 400-year eras, then undo the shift.
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

/// A duration in whole milliseconds, saturating rather than wrapping: a record
/// that cannot say how long something took says the largest it can.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Why a record could not be written or read. Every one of them names the file
/// that failed, so a run that cannot record says which one to fix.
#[derive(Debug)]
pub enum RecordError {
    /// A file of the session's records could not be written.
    Unwritable {
        /// The path that would not be written.
        path: PathBuf,
        /// Why it would not.
        reason: String,
    },
    /// A document could not be turned into JSON, which no value this crate holds
    /// can do.
    Unserializable {
        /// Why it would not.
        reason: String,
    },
    /// The session directory itself could not be read.
    Unreadable {
        /// The path that would not be read.
        path: PathBuf,
        /// Why it would not.
        reason: String,
    },
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

/// What a run says it is doing, from §14's closed set of five words: there is no
/// sixth, so no status document can report one.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Nothing is being asked and nothing is running.
    Idle,
    /// The run is doing something.
    Working,
    /// The run finished what it was doing.
    Done,
    /// The run is waiting for something it cannot go on without.
    Blocked,
    /// The run failed.
    Error,
}

impl Phase {
    /// The word §14 uses, which is the word a status document writes.
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

/// One status document: what the run said it was doing, in its own words, at the
/// turn it was in (§14).
///
/// The message is a coarse line and never the command, the prompt or the answer;
/// it is scrubbed like every other record field anyway, because a rule about
/// what is worth redacting is a rule and this is a type.
///
/// It reads back as it was written, so a front end showing a past run draws the
/// status documents from the facts they carry and from no second rendering of
/// them.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Status {
    /// Which of the five states the run was in.
    phase: Phase,
    /// The one coarse line, in the run's words.
    message: Redacted,
    /// The turn the run was in.
    turn: u32,
    /// The workspace's name, and never the whole path.
    workspace: String,
}

impl Status {
    /// Which of the five states the run was in.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The one coarse line, in the run's words.
    pub fn message(&self) -> &Redacted {
        &self.message
    }

    /// The turn the run was in.
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The workspace's name, which is the last component and never the path.
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
}

/// The name of `workspace`: its last component, which is what §14 puts in a
/// status line, and the whole path when it has no name to give — the root.
fn name_of(workspace: &Path) -> String {
    match workspace.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => workspace.display().to_string(),
    }
}

/// Writes one value as one line, whole, in a single call: nothing is buffered
/// and nothing is half written, so a kill lands between lines and never inside
/// one.
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

/// A file opened for appending, made when it is not there yet: a record that
/// already exists is added to and never truncated.
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

/// What a session's transcript reads back as: the entries this build knows, in
/// the order they were written, and how many lines it could not read.
///
/// Reading is more tolerant than writing is, on purpose (ADR-0007): a line whose
/// `kind` this build does not know, or whose fields it cannot fill, is skipped
/// and counted, and the rest of the transcript still loads. A session written by
/// a newer `sindri` is therefore still worth opening.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Transcript {
    /// The entries, oldest first.
    entries: Vec<Entry>,
    /// How many lines were not entries this build could read.
    skipped: usize,
}

impl Transcript {
    /// The entries, oldest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// How many lines were not entries this build could read.
    pub fn skipped(&self) -> usize {
        self.skipped
    }
}

/// The lines of `path`, each one as the file spells it, and only the ones that
/// are one JSON value: the raw reading of a line file, for a reader that hands
/// the bytes on rather than decoding them into this build's types.
///
/// The skip rule is the transcript's, one step looser: a line that is not JSON at
/// all — half of one, from a run killed mid-write — is no shape to hand on, and a
/// file that was never written is no lines rather than a failure.
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

/// Reads the status documents in `directory`, skipping what this build cannot
/// read: a session that said nothing yet reads as no documents, and a line
/// written by a newer `sindri` costs that line and nothing else, exactly as a
/// line of the transcript does (ADR-0007).
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

/// Reads the transcript in `directory`, skipping what this build cannot read: a
/// session with no entries yet reads as no entries, and a line written by a
/// newer `sindri` costs that line and nothing else.
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

/// The path of `directory`'s session document: §13's `session.json`, which is
/// what a run was and how it ended. A reader is handed the path rather than
/// spelling the layout itself, so one file's name is written down once.
pub fn document_path(directory: &Path) -> PathBuf {
    directory.join(SESSION)
}

/// The path of `directory`'s transcript: §13's `transcript.jsonl`, one entry per
/// line, in order.
pub fn transcript_path(directory: &Path) -> PathBuf {
    directory.join(TRANSCRIPT)
}

/// The path of `directory`'s status documents: §13's `status.json`, one document
/// per line, as the run said what it was doing (§14).
pub fn status_path(directory: &Path) -> PathBuf {
    directory.join(STATUS)
}

/// One session as a reader finds it: the directory it lives in, and the facts its
/// `session.json` states that listing a run and reading one back have to say —
/// what it asked, where it stood, when it started, how far it got and why it
/// stopped, which is `running` until a stop reason lands (§13).
///
/// The whole document is the file's and [`crate::session`]'s; this is the reading
/// a person is shown, so the policy block and the dials are not copied here. A
/// field this build does not know costs nothing (ADR-0007), and a field the file
/// does not carry is the nothing it is: no branch, no commit, no end.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Recorded {
    /// The directory the session lives in, which is the one fact its document
    /// does not state.
    #[serde(skip)]
    directory: PathBuf,
    /// The GUID the directory is named by.
    session: String,
    /// The provider dialect the run asked.
    #[serde(rename = "provider")]
    wire: Wire,
    /// The model it asked for.
    model: String,
    /// How its commands were confined.
    enforcement: Enforcement,
    /// The one directory the run stood in.
    workspace: PathBuf,
    /// The branch the workspace's checkout was on, when it was on one.
    branch: Option<String>,
    /// The commit the workspace's checkout was at, when it was at one.
    commit: Option<String>,
    /// When the run started, as an RFC 3339 instant in UTC.
    start: String,
    /// When it stopped, in the same shape, once a stop reason landed.
    end: Option<String>,
    /// How many turns it started.
    turns: u32,
    /// What its asks spent.
    usage: Usage,
    /// Why it stopped: `running` until a stop reason lands, then §4's word.
    stopped: String,
}

impl Recorded {
    /// `…/sessions/<guid>`, where the session's three files are.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The GUID the directory is named by, which is what `show` is given.
    pub fn guid(&self) -> &str {
        &self.session
    }

    /// The provider dialect the run asked.
    pub fn wire(&self) -> Wire {
        self.wire
    }

    /// The model it asked for.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// How its commands were confined.
    pub fn enforcement(&self) -> Enforcement {
        self.enforcement
    }

    /// The one directory the run stood in.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// The branch the workspace's checkout was on, when it was on one.
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// The commit the workspace's checkout was at, when it was at one.
    pub fn commit(&self) -> Option<&str> {
        self.commit.as_deref()
    }

    /// When the run started, as an RFC 3339 instant in UTC.
    pub fn start(&self) -> &str {
        &self.start
    }

    /// When it stopped, once a stop reason landed, and nothing while it runs.
    pub fn end(&self) -> Option<&str> {
        self.end.as_deref()
    }

    /// How many turns it started.
    pub fn turns(&self) -> u32 {
        self.turns
    }

    /// What its asks spent.
    pub fn usage(&self) -> Usage {
        self.usage
    }

    /// Why it stopped: `running` until a stop reason lands, then §4's word. A
    /// session superseded by `/new` keeps saying `running`, because it did not
    /// stop — it was left (§13).
    pub fn stopped(&self) -> &str {
        &self.stopped
    }
}

/// Every session under `root`, newest first: the document's own `start` is what
/// orders them, so a listing reads as the runs happened and not as their GUIDs
/// were drawn, and two sessions that started in the same second are ordered by
/// GUID so two listings of one state home agree.
///
/// A state home that holds no sessions reads as no sessions, and a directory that
/// holds no document this build can read is not a session to list: a reader of
/// past runs says what it can read rather than refusing the whole state home for
/// one directory in it.
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

/// The one session `id` names: a whole GUID, or a prefix of exactly one.
///
/// A whole GUID is a prefix of itself and of no other GUID, so one rule reads both
/// — and a prefix that more than one session carries names none of them, because
/// a reader that guessed which was meant would read the wrong run.
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

/// Why an id names no one session: nothing carries it, or more than one thing
/// does. Both are §15's usage error, because an id is part of a command line and
/// a command line that names no one session cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupError {
    /// `id` is the prefix of more than one session, and so names none of them.
    Ambiguous {
        /// The id as it was typed.
        id: String,
        /// Every GUID it is a prefix of, in listing order.
        candidates: Vec<String>,
    },
    /// No session carries `id`, as a whole GUID or as a prefix.
    Absent {
        /// The id as it was typed.
        id: String,
    },
}

impl LookupError {
    /// The exit code §15 gives this, which is the usage error both take.
    pub fn exit_code(&self) -> u8 {
        2
    }

    /// Every GUID an ambiguous id is a prefix of, and nothing for an id no
    /// session carries: what a refusal lists so the next attempt can be exact.
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

/// The session `directory` records, and nothing when it records none: no document
/// is no session, and a document this build cannot read is one it steps over for
/// the same reason [`read_transcript`] steps over a line it cannot read.
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
    /// The transcript `text` spells, which is the reading rule and not the
    /// file: a line this build cannot read is counted and stepped over.
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
