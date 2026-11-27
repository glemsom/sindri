use crate::config::Environment;
use crate::records::timestamp;
use crate::session::state_directory;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const RECALL: &str = "recall.jsonl";
const CEILING: usize = 4096;
const CAPACITY: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Entry {
    at: String,
    prompt: String,
}

impl Entry {
    pub fn at(&self) -> &str {
        &self.at
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }
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
            Some(path) => read(path),
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

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn append(&mut self, prompt: &str) {
        if prompt.len() > CEILING {
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
    if entries.len() <= CAPACITY {
        return false;
    }
    entries.drain(..entries.len() - CAPACITY);
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

fn written(path: &Path, entry: &Entry) -> Option<()> {
    let said = serde_json::to_string(entry).ok()?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).ok()?;
    }
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)
        .ok()?;
    file.write_all(format!("{said}\n").as_bytes()).ok()
}

fn rewritten(path: &Path, entries: &[Entry]) -> Option<()> {
    let mut said = String::new();
    for entry in entries {
        said.push_str(&serde_json::to_string(entry).ok()?);
        said.push('\n');
    }
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).ok()?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .ok()?;
    file.write_all(said.as_bytes()).ok()
}

#[cfg(test)]
mod tests;
