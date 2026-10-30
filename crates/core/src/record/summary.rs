//! Every record a run left, as one list a front end can draw:
//! `/sessions` reads `session.json` for what a run was and the first thing the
//! human wrote for what it was asked. Reading is not resuming — nothing here
//! writes, and nothing starts a run.

use std::io::{BufRead, BufReader};
use std::path::Path;

use super::entry::{Entry, Meta};

/// How far into a transcript the first prompt is looked for: a run opens with
/// the prompt, so a record whose first hundred lines hold no user text has no
/// prompt to show and a list must not read a megabyte to find that out.
const PROMPT_LINES: usize = 100;

/// One record as a list needs it: what the run was, how it ended, and what it
/// was first asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub id: String,
    pub started_ms: u64,
    pub model: String,
    pub turns: Option<u32>,
    /// `final`, `max_turns`, `cancelled`, `error` — or `running`, said exactly
    /// as the record says it: a record still saying `running` is a run that
    /// never finished, and a list that guessed otherwise would be lying.
    pub stopped: String,
    pub prompt: Option<String>,
}

/// Every record under `sessions`, newest first. A directory that is not a
/// record is skipped rather than fatal: one unreadable run does not hide the
/// rest, and a listing is a read of the state directory and nothing else.
pub fn summaries(sessions: &Path) -> Vec<Summary> {
    let Ok(entries) = std::fs::read_dir(sessions) else {
        return Vec::new();
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
    rows
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
