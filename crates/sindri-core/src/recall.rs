use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Environment;
use crate::records::{read_lines, timestamp};
use crate::session;

const APPLICATION: &str = "sindri";

const FILE: &str = "recall.jsonl";

/// How many prompts a recall carries: past this the oldest goes, and a launch that finds more
/// rewrites the file.
pub const KEPT: usize = 200;

/// The longest prompt a recall keeps, in bytes. A pasted log is not a stroke an arrow key should
/// have to walk past, so one is never stored at all.
pub const LONGEST: usize = 4_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

/// The reader's own prompts across launches, in the order they were sent. Not a `Run`'s history
/// and not a session's record: those are what a run did, and this is what the reader wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recall {
    file: Option<PathBuf>,
    entries: Vec<Entry>,
}

impl Recall {
    /// A recall of the file the state directory holds, empty and never a refusal when there is
    /// no state home, no file, or a line the reader broke by hand.
    pub fn load(environment: &impl Environment) -> Recall {
        let Some(file) = file_of(environment) else {
            return Recall::default();
        };
        let mut entries: Vec<Entry> = read_lines(&file)
            .unwrap_or_default()
            .iter()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if entries.len() > KEPT {
            let over = entries.len() - KEPT;
            entries.drain(..over);
            rewrite(&file, &entries);
        }
        Recall {
            file: Some(file),
            entries,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The prompt `back` prompts before the newest, `0` being the newest itself.
    pub fn older(&self, back: usize) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .nth(back)
            .map(|entry| entry.prompt.as_str())
    }

    pub fn path(&self) -> Option<&Path> {
        self.file.as_deref()
    }

    /// Keeps one prompt, appended to the file as it is sent. Nothing about a store that cannot
    /// write is a refusal: the prompt is still the launch's to hand back.
    pub fn send(&mut self, prompt: &str) {
        if prompt.is_empty() || prompt.len() > LONGEST {
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
            at: timestamp(std::time::SystemTime::now()),
            prompt: prompt.to_owned(),
        };
        if let Some(file) = self.file.as_deref() {
            append(file, &entry);
        }
        self.entries.push(entry);
    }
}

fn file_of(environment: &impl Environment) -> Option<PathBuf> {
    session::state_home(environment)
        .ok()
        .map(|home| home.join(APPLICATION).join(FILE))
}

fn append(path: &Path, entry: &Entry) {
    let Some(directory) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(directory).is_err() {
        return;
    }
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| {
            let mut line = serde_json::to_vec(entry).unwrap_or_default();
            line.push(b'\n');
            file.write_all(&line)
        });
    let _ = written;
}

fn rewrite(path: &Path, entries: &[Entry]) {
    let Some(directory) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(directory).is_err() {
        return;
    }
    let text: String = entries
        .iter()
        .filter_map(|entry| serde_json::to_string(entry).ok())
        .map(|line| line + "\n")
        .collect();
    let staged = tempfile::Builder::new()
        .prefix(".recall.")
        .suffix(".staged")
        .tempfile_in(directory);
    let Ok(staged) = staged else {
        return;
    };
    let _ = staged
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600));
    let _ = staged.as_file().write_all(text.as_bytes());
    let _ = staged.persist(path);
}
