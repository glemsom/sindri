//! One eval's run, as an eval checks it: what the shell saw, what the model was
//! told, and the files the run left behind. Nothing here reaches inside the
//! binary; everything is a value a process produced.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde_json::Value;
use sindri_harness::{MockProvider, Reply, Request};

use crate::case::Case;
use crate::workspace::Workspace;

/// The result of one scripted run.
#[derive(Debug)]
pub struct Run {
    /// What the shell saw.
    pub exit_code: i32,
    /// The answer, or the JSON object, on stdout.
    pub stdout: String,
    /// Whatever went to stderr, which a clean run leaves empty.
    pub stderr: String,
    /// Every request the binary made to the mock provider, in order. Empty in a
    /// live run, where there is no mock to ask.
    pub requests: Vec<Request>,
    /// The fixture repository this run worked in.
    pub workspace: PathBuf,
    /// The run's state directory, where its session record was written.
    pub state: PathBuf,
    /// Whether the run asked a real provider: a live run cannot assert on the
    /// bodies a recorded one replays.
    pub live: bool,
}

impl Run {
    /// The run's own JSON object, when it printed one.
    pub fn json(&self) -> Option<Value> {
        serde_json::from_str(self.stdout.trim()).ok()
    }

    /// A field of the JSON object, as text.
    pub fn field(&self, name: &str) -> Option<String> {
        let value = self.json()?;
        match value.get(name)? {
            Value::String(text) => Some(text.clone()),
            other => Some(other.to_string()),
        }
    }

    /// The whole request body the model was sent on turn `n`, counted from 1.
    pub fn request(&self, n: usize) -> Option<&str> {
        self.requests
            .get(n - 1)
            .map(|request| request.body.as_str())
    }

    /// The one session directory the run recorded itself in.
    pub fn session_dir(&self) -> Option<PathBuf> {
        let root = self.state.join("sindri/sessions");
        let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
            .ok()?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect();
        found.sort();
        found.pop()
    }

    /// `session.json`, as the record wrote it.
    pub fn session_meta(&self) -> Option<Value> {
        let text = std::fs::read_to_string(self.session_dir()?.join("session.json")).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The transcript, one JSON line per entry.
    pub fn transcript(&self) -> Vec<Value> {
        let Some(dir) = self.session_dir() else {
            return Vec::new();
        };
        std::fs::read_to_string(dir.join("transcript.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// Whether `path` exists, without following it: an escape that landed is a
/// file, and the message must not hide which one.
pub fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// The first line of a stream, for a finding that has to fit on one line.
pub fn first_line(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.is_empty() {
        "<empty>".to_string()
    } else {
        line.chars().take(160).collect()
    }
}

/// The provider variables a host may have exported; an eval must never inherit
/// them, because a credential or an unsafe setting is exactly what the suite is
/// checking is not in play.
const SCRUBBED: &[&str] = &[
    "SINDRI_PROVIDER",
    "SINDRI_MODEL",
    "SINDRI_API_KEY",
    "SINDRI_BASE_URL",
    "SINDRI_MAX_TURNS",
    "SINDRI_TIMEOUT",
    "SINDRI_THINKING",
    "SINDRI_EXTRA_WRITABLE_PATHS",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
];

/// Runs one eval's script: the real binary, in its own workspace, with the
/// recorded conversation replayed or a live provider asked.
pub fn execute(bin: &Path, ws: &Workspace, case: &Case, live: bool) -> Result<Run, String> {
    let mock = if live {
        None
    } else {
        Some(MockProvider::start(replies(case)?))
    };

    let mut command = std::process::Command::new(bin);
    command
        .current_dir(&ws.repo)
        .args(["-b", case.prompt, "--format", "json"])
        .args(case.args)
        .env("XDG_STATE_HOME", &ws.state)
        .env("XDG_CONFIG_HOME", &ws.config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in SCRUBBED {
        command.env_remove(name);
    }
    if let Some(mock) = &mock {
        command
            .env("SINDRI_PROVIDER", "anthropic")
            .env("SINDRI_API_KEY", "sk-ant-eval-not-a-real-key")
            .env("SINDRI_MODEL", "claude-sonnet-4-6")
            .env("SINDRI_BASE_URL", mock.base_url());
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn {}: {error}", bin.display()))?;
    if let Some(file) = case.stdin_file {
        let context = std::fs::read_to_string(Path::new(crate::workspace::FIXTURES).join(file))
            .map_err(|error| format!("read {file}: {error}"))?;
        let mut stdin = child.stdin.take().ok_or("the child has no stdin")?;
        stdin
            .write_all(context.as_bytes())
            .map_err(|error| format!("pipe the diff: {error}"))?;
    }
    drop(child.stdin.take());
    let output = child
        .wait_with_output()
        .map_err(|error| format!("wait for {}: {error}", bin.display()))?;

    Ok(Run {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        requests: mock.map(|mock| mock.requests()).unwrap_or_default(),
        workspace: ws.repo.clone(),
        state: ws.state.clone(),
        live,
    })
}

/// The recorded conversation for `case`, in the order the provider answered:
/// the wire bodies, so a replay proves our client and our loop together.
fn replies(case: &Case) -> Result<Vec<Reply>, String> {
    let dir = Path::new(crate::workspace::FIXTURES).join(case.recorded);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    if names.is_empty() {
        return Err(format!("{} holds no recorded reply", dir.display()));
    }
    Ok(names
        .iter()
        .map(|name| {
            let stem = name.trim_end_matches(".json");
            Reply::from_file(dir.display().to_string().as_str(), stem, 200)
        })
        .collect())
}
