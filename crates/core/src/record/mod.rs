//! The record every run leaves (ADR-0006): `session.json` for metadata,
//! `transcript.jsonl` for what happened in order, and `status.json` for
//! bubblewrap's own documents as they arrive. Written as the run goes, so a
//! cancelled or crashed run still leaves readable evidence — nothing is buffered
//! until the end, because the end is exactly what a crash takes away.
//!
//! Writing is best effort: a full disk must not turn an answer into a failure.
//! `start` and `finish` do report trouble, because those are the two moments a
//! front end can still say something.

mod entry;
mod redact;
mod stored;

pub use entry::{Entry, Meta};
pub use redact::Redaction;
pub use stored::{ShowError, Stored};

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::agent::Report;
use crate::exec::StatusSink;
use crate::paths::SessionDir;
use crate::provider::{Call, ToolOutcome};
use crate::sandbox::Enforcement;
use entry::Entry as Line;

/// Who is running, and where: everything the metadata needs before a turn is
/// asked (ADR-0006).
#[derive(Debug, Clone)]
pub struct Start {
    /// The provider that will be asked.
    pub provider: String,
    /// The model that was configured, whether or not it answers.
    pub model: String,
    /// Whether commands are sandboxed or run with the user's own privileges.
    pub enforcement: Enforcement,
}

/// How a run's enforcement is written down.
fn enforcement(enforcement: Enforcement) -> &'static str {
    match enforcement {
        Enforcement::Bubblewrap => "bubblewrap",
        Enforcement::YoloUnsafe => "unsafe",
    }
}

/// One run's record, held open for as long as the run lasts.
pub struct Recorder {
    root: PathBuf,
    redaction: Redaction,
    inner: Mutex<Inner>,
}

/// The files and the metadata a recorder owns, behind one lock so the status
/// thread and the turn loop can write without a sequence number of their own.
struct Inner {
    meta: Meta,
    transcript: File,
    status: File,
}

impl Recorder {
    /// Opens the record: metadata written once, transcript and status created
    /// empty. A session whose record cannot be written cannot be a run.
    pub fn start(session: &SessionDir, start: Start, redaction: Redaction) -> io::Result<Self> {
        let root = session.root().to_path_buf();
        std::fs::create_dir_all(&root)?;
        let meta = Meta {
            id: session.id().to_string(),
            version: sindri_types::VERSION_LONG.to_string(),
            provider: start.provider,
            model: start.model,
            enforcement: enforcement(start.enforcement).to_string(),
            pid: std::process::id(),
            started_ms: now_ms(),
            ended_ms: None,
            duration_ms: None,
            answered_model: None,
            turns: None,
            tool_calls: None,
            stopped: "running".to_string(),
            usage: None,
            error: None,
        };
        write_meta(&root, &meta)?;
        let transcript = open(root.join("transcript.jsonl"))?;
        let status = open(root.join("status.json"))?;
        let recorder = Recorder {
            root,
            redaction,
            inner: Mutex::new(Inner {
                meta,
                transcript,
                status,
            }),
        };
        Ok(recorder)
    }

    /// The directory this record lives in.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The question the run was asked.
    pub fn user(&self, text: &str) {
        self.note(Line::User {
            text: self.redaction.apply(text),
        });
    }

    /// One turn's reply from the model.
    pub fn assistant(&self, turn: u32, text: &str, reasoning: Option<&str>) {
        self.note(Line::Assistant {
            turn,
            text: self.redaction.apply(text),
            reasoning: reasoning.map(|text| self.redaction.apply(text)),
        });
    }

    /// One call, written before it runs, so a command killed mid-flight is still
    /// on the record.
    pub fn tool_call(&self, turn: u32, call: &Call) {
        let (command, problem) = match call.command() {
            Ok(command) => (Some(self.redaction.apply(&command)), None),
            Err(reason) => (None, Some(self.redaction.apply(&reason))),
        };
        self.note(Line::ToolCall {
            turn,
            call_id: call.id.clone(),
            tool: call.name.clone(),
            command,
            problem,
        });
    }

    /// What the tool answered the model.
    pub fn tool_result(&self, turn: u32, outcome: &ToolOutcome) {
        self.note(Line::ToolResult {
            turn,
            call_id: outcome.call_id.clone(),
            content: self.redaction.apply(&outcome.content),
            is_error: outcome.is_error,
        });
    }

    /// The run ended, for the reason the report gives.
    pub fn finish(&self, report: &Report) -> io::Result<()> {
        self.settle(|meta| {
            meta.stopped = report.stopped.as_str().to_string();
            meta.answered_model.clone_from(&report.model);
            meta.turns = Some(report.turns);
            meta.tool_calls = Some(report.tool_calls);
            meta.duration_ms = Some(u64::try_from(report.duration.as_millis()).unwrap_or(u64::MAX));
            meta.usage = (!report.usage.is_empty()).then(|| report.usage.clone());
        })
    }

    /// The run failed, with the message the shell was told.
    pub fn failed(&self, message: &str) -> io::Result<()> {
        let message = self.redaction.apply(message);
        self.settle(move |meta| {
            meta.stopped = "error".to_string();
            meta.error = Some(message);
        })
    }

    /// Rewrites the metadata once the run has an end to report, atomically: a
    /// reader never sees half a document.
    fn settle(&self, change: impl FnOnce(&mut Meta)) -> io::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("the record was poisoned"))?;
        change(&mut inner.meta);
        inner.meta.ended_ms = Some(now_ms());
        write_meta(&self.root, &inner.meta)
    }

    /// Appends one line. A write that fails is not worth failing a run over, and
    /// a short record is still evidence of everything up to that point.
    fn note(&self, line: Line) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let _ = writeln!(inner.transcript, "{}", json(&line));
    }
}

impl StatusSink for Recorder {
    /// One of bubblewrap's status documents, verbatim where it is not JSON:
    /// what a post-mortem needs is what bwrap actually said.
    fn document(&self, call: u64, document: &str) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        let line = match serde_json::from_str::<serde_json::Value>(document) {
            Ok(parsed) => serde_json::json!({ "call": call, "document": parsed }),
            Err(_) => serde_json::json!({ "call": call, "line": document }),
        };
        let _ = writeln!(inner.status, "{}", json(&line));
    }
}

/// One value as JSON. Every value written here is a struct or an object of
/// strings and numbers, which `serde_json` cannot refuse.
fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

fn open(path: PathBuf) -> io::Result<File> {
    File::options().create(true).append(true).open(path)
}

/// Writes the metadata beside itself, then renames it into place: a reader sees
/// the old document or the new one, never a half-written one.
fn write_meta(root: &Path, meta: &Meta) -> io::Result<()> {
    let staging = root.join("session.json.tmp");
    std::fs::write(&staging, format!("{}\n", json(meta)))?;
    std::fs::rename(&staging, root.join("session.json"))
}

/// Now, in milliseconds since the Unix epoch: the record's only clock.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
