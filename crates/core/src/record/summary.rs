//! Every record a run left, as one list a front end can draw: `/sessions` and
//! `sindri sessions list` read `session.json` for what a run was and what it
//! spent, and the first thing the human wrote for what it was asked. Reading is
//! not resuming — nothing here writes, and nothing starts a run.
//!
//! A row's JSON is the struct's own field order, so the object a script reads
//! and the row a person reads cannot disagree about which fact comes where.

use std::io::{self, BufRead, BufReader};
use std::path::Path;

use super::entry::{Entry, Meta};
use crate::provider::Usage;

/// How far into a transcript the first prompt is looked for: a run opens with
/// the prompt, so a record whose first hundred lines hold no user text has no
/// prompt to show and a list must not read a megabyte to find that out.
const PROMPT_LINES: usize = 100;

/// One record as a list needs it: what the run was, how it ended, what it spent
/// and what it was first asked. Serialized as it stands, in this order, for
/// `--json`: a fact the record does not have is left out rather than sent as
/// `null`, the rule `--format json` already keeps.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Summary {
    pub id: String,
    pub started_ms: u64,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<u32>,
    /// `final`, `max_turns`, `cancelled`, `error` — or `running`, said exactly
    /// as the record says it: a record still saying `running` never finished.
    pub stopped: String,
    /// The run's total, as its own metadata states it: absent when nobody
    /// measured it, which is not the same as free.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

/// Every record under `sessions`, newest first, for a reader with no one to
/// tell: a directory it cannot read is an empty list.
pub fn summaries(sessions: &Path) -> Vec<Summary> {
    listing(sessions).unwrap_or_default()
}

/// Every record under `sessions`, newest first, or why the directory could not
/// be read. A directory that is not there is a state directory no run has
/// written to yet, so it holds no records rather than being a failure. A
/// directory inside it that is not a record is skipped rather than fatal: one
/// unreadable run does not hide the rest, and a listing is a read of the state
/// directory and nothing else.
pub fn listing(sessions: &Path) -> io::Result<Vec<Summary>> {
    let entries = match std::fs::read_dir(sessions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut rows: Vec<Summary> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| summary_of(&entry.path()))
        .collect();
    // Two runs that started in the same millisecond are ordered by id, so a
    // list is stable rather than however the directory happened to be read.
    rows.sort_by(|a, b| {
        b.started_ms
            .cmp(&a.started_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(rows)
}

fn summary_of(dir: &Path) -> Option<Summary> {
    let text = std::fs::read_to_string(dir.join("session.json")).ok()?;
    let meta: Meta = serde_json::from_str(&text).ok()?;
    Some(Summary {
        prompt: first_prompt(&dir.join("transcript.jsonl")),
        // The directory's own name, not the id inside the document: a name a
        // directory listing handed us cannot walk out of the state directory,
        // and a record is the directory it lives in.
        id: dir.file_name()?.to_str()?.to_string(),
        started_ms: meta.started_ms,
        model: meta.model,
        turns: meta.turns,
        stopped: meta.stopped,
        usage: meta.usage,
    })
}

/// The first thing the human asked, whatever else the transcript holds first.
/// A line that is not JSON stops the search: a transcript is written in order,
/// and what follows a broken line cannot be trusted to be what came next.
fn first_prompt(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    for line in BufReader::new(file).lines().take(PROMPT_LINES) {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Entry>(&line) {
            Ok(Entry::User { text }) => return Some(text),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}
