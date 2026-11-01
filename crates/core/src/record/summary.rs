//! Every record a run left, as one list a front end can draw: `sindri sessions
//! list` reads `session.json` for what a run was and what it spent, and the
//! first thing the human wrote for what it was asked. The frame reads no
//! listing: `sindri sessions show` and `/export` read one record whole, out of
//! the same files. Reading is not resuming — nothing here writes, and nothing
//! starts a run.
//!
//! A row's JSON is the struct's own field order, so the object a script reads
//! and the row a person reads cannot disagree about which fact comes where.

use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

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

/// Every record under `sessions`, newest first, or why the directory could not
/// be read. A directory that is not there is a state directory no run has
/// written to yet, so it holds no records rather than being a failure. A
/// directory inside it that is not a record is skipped rather than fatal: one
/// unreadable run does not hide the rest, and a listing is a read of the state
/// directory and nothing else.
pub fn listing(sessions: &Path) -> io::Result<Vec<Summary>> {
    listing_limited(sessions, None)
}

/// The newest `limit` rows of [`listing`], or every row when there is no limit:
/// what a front end printing a slice of the list asks for.
///
/// The limit is what keeps a listing cheap. Every row's metadata is still read —
/// the sort cannot know which runs are newest before it has them — but a
/// transcript is opened only for the rows the limit keeps, so
/// `sindri sessions list --limit 5` scans five prompts rather than five hundred.
pub fn listing_limited(sessions: &Path, limit: Option<usize>) -> io::Result<Vec<Summary>> {
    let mut rows = metadata(sessions)?;
    // Two runs that started in the same millisecond are ordered by id, so a
    // list is stable rather than however the directory happened to be read.
    rows.sort_by(|a, b| {
        b.summary
            .started_ms
            .cmp(&a.summary.started_ms)
            .then_with(|| a.summary.id.cmp(&b.summary.id))
    });
    rows.truncate(limit.unwrap_or(usize::MAX));
    Ok(rows.into_iter().map(Row::prompted).collect())
}

/// One record read as far as its metadata goes, with the directory it came from:
/// the prompt is a second read of the transcript, and the row kept has to know
/// where to make it.
struct Row {
    dir: PathBuf,
    summary: Summary,
}

impl Row {
    /// The same row with the first thing the human asked, read now because this
    /// row survived the limit.
    fn prompted(self) -> Summary {
        Summary {
            prompt: first_prompt(&self.dir.join("transcript.jsonl")),
            ..self.summary
        }
    }
}

/// Every record's metadata, in the order the directory handed them over.
fn metadata(sessions: &Path) -> io::Result<Vec<Row>> {
    let entries = match std::fs::read_dir(sessions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| metadata_of(&entry.path()))
        .collect())
}

fn metadata_of(dir: &Path) -> Option<Row> {
    let text = std::fs::read_to_string(dir.join("session.json")).ok()?;
    let meta: Meta = serde_json::from_str(&text).ok()?;
    Some(Row {
        dir: dir.to_path_buf(),
        summary: Summary {
            // The directory's own name, not the id inside the document: a name
            // a directory listing handed us cannot walk out of the state
            // directory, and a record is the directory it lives in.
            id: dir.file_name()?.to_str()?.to_string(),
            started_ms: meta.started_ms,
            model: meta.model,
            turns: meta.turns,
            stopped: meta.stopped,
            usage: meta.usage,
            prompt: None,
        },
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
