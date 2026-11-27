use crate::config::Environment;
use crate::records::timestamp;
use crate::session::state_directory;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const RECALL: &str = "recall.jsonl";
const PROMPT_BYTES: usize = 4096;
const ENTRIES_KEPT: usize = 500;
const MODE: u32 = 0o600;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Entry {
    at: String,
    prompt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recall {
    path: Option<PathBuf>,
    entries: Vec<Entry>,
}

impl Recall {
    pub fn load(environment: &impl Environment) -> Recall {
        let path = state_directory(environment).ok().map(|at| at.join(RECALL));
        let mut entries = match path.as_deref() {
            Some(path) => {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(MODE));
                read(path)
            }
            None => Vec::new(),
        };
        if trimmed_to_capacity(&mut entries)
            && let Some(path) = path.as_deref()
        {
            let _ = rewritten(path, &entries);
        }
        Recall { path, entries }
    }

    pub fn none() -> Recall {
        Recall {
            path: None,
            entries: Vec::new(),
        }
    }

    pub fn prompt(&self, back: usize) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .nth(back)
            .map(|entry| entry.prompt.as_str())
    }

    pub fn prompts(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.prompt.as_str())
    }

    pub fn append(&mut self, prompt: &str) {
        if prompt.len() > PROMPT_BYTES {
            return;
        }
        if self
            .entries
            .last()
            .is_some_and(|entry| entry.prompt == prompt)
        {
            return;
        }
        let entry = Entry {
            at: timestamp(SystemTime::now()),
            prompt: prompt.to_owned(),
        };
        if let Some(path) = self.path.as_deref() {
            let _ = written(path, &entry);
        }
        self.entries.push(entry);
        trimmed_to_capacity(&mut self.entries);
    }
}

fn trimmed_to_capacity(entries: &mut Vec<Entry>) -> bool {
    if entries.len() <= ENTRIES_KEPT {
        return false;
    }
    entries.drain(..entries.len() - ENTRIES_KEPT);
    true
}

fn read(path: &Path) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn opened(path: &Path, truncate: bool) -> Option<File> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).ok()?;
    }
    OpenOptions::new()
        .write(true)
        .append(!truncate)
        .truncate(truncate)
        .create(true)
        .mode(MODE)
        .open(path)
        .ok()
}

fn written(path: &Path, entry: &Entry) -> Option<()> {
    let said = serde_json::to_string(entry).ok()?;
    opened(path, false)?
        .write_all(format!("{said}\n").as_bytes())
        .ok()
}

fn rewritten(path: &Path, entries: &[Entry]) -> Option<()> {
    let mut said = String::new();
    for entry in entries {
        said.push_str(&serde_json::to_string(entry).ok()?);
        said.push('\n');
    }
    opened(path, true)?.write_all(said.as_bytes()).ok()
}

#[cfg(test)]
mod tests;
