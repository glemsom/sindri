//! Reading a record back: `sindri sessions show <id>` and, later, a TUI's
//! session restore, from the same files the run wrote.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use super::entry::{Entry, Meta};
use crate::paths::{SessionDir, state_dir};

#[derive(Debug)]
pub enum ShowError {
    /// The id is not a session GUID: never a path, so a lookup cannot escape.
    BadId {
        id: String,
    },
    Missing {
        path: PathBuf,
    },
    Unreadable {
        path: PathBuf,
        source: io::Error,
    },
    /// Only ever the metadata: without `session.json` there is no record at
    /// all. A transcript line that does not parse is counted in
    /// [`Stored::unread`] instead, because what came before it is still
    /// evidence.
    Corrupt {
        path: PathBuf,
        detail: String,
    },
}

impl fmt::Display for ShowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShowError::BadId { id } => write!(f, "`{id}` is not a session id"),
            ShowError::Missing { path } => write!(f, "no session recorded in {}", path.display()),
            ShowError::Unreadable { path, source } => {
                write!(f, "{} cannot be read: {source}", path.display())
            }
            ShowError::Corrupt { path, detail } => {
                write!(f, "{} is not a record: {detail}", path.display())
            }
        }
    }
}

impl std::error::Error for ShowError {}

pub struct Stored {
    pub meta: Meta,
    pub entries: Vec<Entry>,
    /// Transcript lines that could not be read: a crash cuts the line it was
    /// writing, and a record short of its tail is evidence of everything up to
    /// the cut. Counted rather than fatal, said rather than silent.
    pub unread: usize,
    /// `status.json`'s lines verbatim: bubblewrap's own documents are not ours
    /// to type, and a post-mortem wants what bwrap actually said.
    pub status: Vec<String>,
}

impl Stored {
    pub fn load(id: &str) -> Result<Self, ShowError> {
        if !is_session_id(id) {
            return Err(ShowError::BadId { id: id.to_string() });
        }
        let session = state_dir()
            .map(|state| SessionDir::at(state.join("sessions").join(id)))
            .map_err(|source| ShowError::Unreadable {
                path: PathBuf::from("<state dir>"),
                source,
            })?;
        Stored::load_in(&session)
    }

    pub fn load_in(session: &SessionDir) -> Result<Self, ShowError> {
        let root = session.root();
        let meta = read(&root.join("session.json"))?;
        let meta: Meta = serde_json::from_str(&meta).map_err(|error| ShowError::Corrupt {
            path: root.join("session.json"),
            detail: error.to_string(),
        })?;

        let path = root.join("transcript.jsonl");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(source) => return Err(ShowError::Unreadable { path, source }),
        };
        let mut entries = Vec::new();
        let mut unread = 0;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            match serde_json::from_str(line) {
                Ok(entry) => entries.push(entry),
                Err(_) => unread += 1,
            }
        }
        let status = read_lines(&root.join("status.json"))?;
        Ok(Stored {
            meta,
            entries,
            unread,
            status,
        })
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            "session {} ({}/{}); recorded by sindri {}\n",
            self.meta.id, self.meta.provider, self.meta.model, self.meta.version
        );
        out.push_str(&format!("sandbox: {}\n", self.meta.enforcement));
        // Only what the record holds: an old record names no tree, and a run
        // outside a repository has no git facts to state.
        if !self.meta.workspace.is_empty() {
            out.push_str(&format!("workspace: {}", self.meta.workspace));
            if let Some(git) = &self.meta.git {
                let dirty = if git.dirty { ", dirty" } else { "" };
                out.push_str(&format!(" ({} @ {}{dirty})", git.branch, git.commit));
            }
            out.push('\n');
        }
        // `max_turns` is zero only in a record written before the dials were
        // kept, and no run obeyed no cap.
        if self.meta.max_turns > 0 {
            let thinking = if self.meta.thinking { "on" } else { "off" };
            out.push_str(&format!(
                "dials: max turns {}, thinking {thinking}\n",
                self.meta.max_turns
            ));
        }
        out.push_str(&format!(
            "started: {} ms since the epoch\n",
            self.meta.started_ms
        ));
        match (self.meta.ended_ms, self.meta.duration_ms) {
            (Some(ended), Some(duration)) => {
                out.push_str(&format!("ended: {ended} after {duration} ms\n"));
            }
            (Some(ended), None) => out.push_str(&format!("ended: {ended}\n")),
            (None, _) => out.push_str("ended: never — the run did not finish\n"),
        }
        out.push_str(&format!("stopped: {}", self.meta.stopped));
        if let (Some(turns), Some(tool_calls)) = (self.meta.turns, self.meta.tool_calls) {
            out.push_str(&format!(
                ", after {turns} turns and {tool_calls} tool calls"
            ));
        }
        out.push('\n');
        if let Some(answered) = &self.meta.answered_model {
            out.push_str(&format!("answered by: {answered}\n"));
        }
        out.push_str(&format!("usage: {}\n", usage(self.meta.usage.as_ref())));
        if let Some(error) = &self.meta.error {
            out.push_str(&format!("error: {error}\n"));
        }
        if self.unread > 0 {
            let line = if self.unread == 1 { "line" } else { "lines" };
            out.push_str(&format!("{} {line} could not be read\n", self.unread));
        }

        for entry in &self.entries {
            match entry {
                Entry::User { text } => {
                    out.push_str("\n--- user ---\n");
                    out.push_str(text);
                    end_line(&mut out, text);
                }
                Entry::Assistant {
                    turn,
                    text,
                    reasoning,
                    ..
                } => {
                    out.push_str(&format!("\n--- assistant (turn {turn}) ---\n"));
                    out.push_str(text);
                    end_line(&mut out, text);
                    if let Some(reasoning) = reasoning {
                        out.push_str(&format!("reasoning: {reasoning}\n"));
                    }
                }
                Entry::ToolCall {
                    turn,
                    call_id,
                    tool,
                    command,
                    problem,
                } => {
                    out.push_str(&format!("\n--- {tool} (turn {turn}) {call_id} ---\n"));
                    match command {
                        Some(command) => {
                            out.push_str(&format!("$ {command}\n"));
                        }
                        None => out.push_str(&format!(
                            "not run: {}\n",
                            problem.as_deref().unwrap_or("the arguments were unusable")
                        )),
                    }
                }
                Entry::Policy { description } => {
                    out.push_str("\n--- policy (from here on) ---\n");
                    out.push_str(description);
                    end_line(&mut out, description);
                }
                Entry::ToolResult {
                    turn,
                    call_id,
                    content,
                    is_error,
                    ..
                } => {
                    let failed = if *is_error { " (failed)" } else { "" };
                    out.push_str(&format!(
                        "\n--- result (turn {turn}) {call_id}{failed} ---\n"
                    ));
                    for line in content.lines() {
                        out.push_str(&format!("  {line}\n"));
                    }
                }
            }
        }

        if !self.status.is_empty() {
            out.push_str("\n--- sandbox status ---\n");
            for line in &self.status {
                out.push_str(line);
                out.push('\n');
            }
        }
        out
    }
}

/// A file's lines, or none at all when the file was never written: a record
/// without a `status.json` is a record whose run had nothing to say.
fn read_lines(path: &Path) -> Result<Vec<String>, ShowError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text.lines().map(str::to_string).collect()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(ShowError::Unreadable {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read(path: &Path) -> Result<String, ShowError> {
    std::fs::read_to_string(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            ShowError::Missing {
                path: path.to_path_buf(),
            }
        } else {
            ShowError::Unreadable {
                path: path.to_path_buf(),
                source,
            }
        }
    })
}

fn end_line(out: &mut String, text: &str) {
    if !text.ends_with('\n') {
        out.push('\n');
    }
}

fn usage(usage: Option<&crate::provider::Usage>) -> String {
    let Some(usage) = usage else {
        return "none reported".to_string();
    };
    let counter = |name: &str, value: Option<u64>| match value {
        Some(value) => format!("{name} {value}"),
        None => format!("{name} unreported"),
    };
    [
        counter("input", usage.input_tokens),
        counter("output", usage.output_tokens),
        counter("cached", usage.cached_input_tokens),
        counter("reasoning", usage.reasoning_tokens),
        counter("total", usage.total_tokens),
    ]
    .join(", ")
}

/// Whether `id` is a session GUID: hex and dashes only, so a lookup can never be
/// a path and `sessions show ../../etc` can never leave the state directory.
fn is_session_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}
