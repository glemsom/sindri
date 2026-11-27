use crate::config::Environment;
use crate::records::timestamp;
use crate::session::state_directory;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const RECALL: &str = "recall.jsonl";

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
        let entries = match path.as_deref() {
            Some(path) => read(path),
            None => Vec::new(),
        };
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
        let entry = Entry {
            at: timestamp(SystemTime::now()),
            prompt: prompt.to_owned(),
        };
        if let Some(path) = self.path.as_deref() {
            let _ = written(path, &entry);
        }
        self.entries.push(entry);
    }
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
        .open(path)
        .ok()?;
    file.write_all(format!("{said}\n").as_bytes()).ok()
}

#[cfg(test)]
mod tests;
