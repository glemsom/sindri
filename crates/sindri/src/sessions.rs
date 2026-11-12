use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use sindri_core::config::Process;
use sindri_core::records::{
    Entry, Recorded, Status, Usage, choose, document_path, read_lines, read_sessions, read_status,
    read_transcript, status_path, transcript_path,
};
use sindri_core::run::Exit;
use sindri_core::session;

use crate::cli::SessionsAction;
use crate::refuse;

/// The exit code for a state home or a session's files that cannot be read.
const SETUP: u8 = 3;

/// The exit code for an id that names no one session, or a `show` with no run to read.
const USAGE: u8 = 2;

pub fn run(action: SessionsAction) -> ExitCode {
    let root = match session::sessions_root(&Process) {
        Ok(root) => root,
        Err(error) => return refuse(&error.to_string(), error.exit_code()),
    };
    let sessions = match read_sessions(&root) {
        Ok(sessions) => sessions,
        Err(error) => return refuse(&error.to_string(), SETUP),
    };
    match action {
        SessionsAction::List => list(&sessions),
        SessionsAction::Show { id, json } => show(&sessions, id.as_deref(), json),
    }
}

fn list(sessions: &[Recorded]) -> ExitCode {
    let mut stdout = io::stdout().lock();
    for recorded in sessions {
        let written = writeln!(
            stdout,
            "{}  {}  {}  {} turns  {} {}",
            recorded.guid(),
            recorded.start(),
            recorded.stopped(),
            recorded.turns(),
            recorded.wire(),
            recorded.model()
        );
        if let Err(error) = written {
            return refuse(&format!("cannot write the listing: {error}"), SETUP);
        }
    }
    if let Err(error) = stdout.flush() {
        return refuse(&format!("cannot write the listing: {error}"), SETUP);
    }
    ExitCode::SUCCESS
}

/// A session `/new` superseded keeps saying `running`: it did not stop, it was left.
fn show(sessions: &[Recorded], id: Option<&str>, json: bool) -> ExitCode {
    let recorded = match id {
        Some(id) => match choose(sessions, id) {
            Ok(recorded) => recorded,
            Err(error) => return refuse(&error.to_string(), error.exit_code()),
        },
        None => match sessions.first() {
            Some(recorded) => recorded,
            None => {
                return refuse(
                    "no session has been recorded yet, so there is none to show",
                    USAGE,
                );
            }
        },
    };

    let read = match json {
        true => own_shape(recorded),
        false => reading(recorded),
    };
    let read = match read {
        Ok(read) => read,
        Err(error) => return refuse(&error, SETUP),
    };
    let mut stdout = io::stdout().lock();
    if let Err(error) = stdout.write_all(read.as_bytes()) {
        return refuse(&format!("cannot write the session: {error}"), SETUP);
    }
    if let Err(error) = stdout.flush() {
        return refuse(&format!("cannot write the session: {error}"), SETUP);
    }
    ExitCode::SUCCESS
}

fn reading(recorded: &Recorded) -> Result<String, String> {
    let directory = recorded.directory();
    let transcript = read_transcript(directory).map_err(|error| error.to_string())?;
    let statuses = read_status(directory).map_err(|error| error.to_string())?;

    let mut lines = vec![
        format!("session {}", recorded.guid()),
        format!("provider {}, model {}", recorded.wire(), recorded.model()),
        format!("enforcement {}", recorded.enforcement()),
        format!("workspace {}", recorded.workspace().display()),
    ];
    if let Some(branch) = recorded.branch() {
        lines.push(format!("branch {branch}"));
    }
    if let Some(commit) = recorded.commit() {
        lines.push(format!("commit {commit}"));
    }
    lines.push(format!("started {}", recorded.start()));
    lines.push(match recorded.end() {
        Some(end) => format!("stopped {}, ended {end}", recorded.stopped()),
        None => format!("stopped {}", recorded.stopped()),
    });
    lines.push(format!("turns {}", recorded.turns()));
    lines.push(spending(recorded.usage()));
    lines.push(match transcript.skipped() {
        0 => format!("transcript {} entries", transcript.entries().len()),
        skipped => format!(
            "transcript {} entries, {skipped} unreadable",
            transcript.entries().len()
        ),
    });
    for entry in transcript.entries() {
        lines.extend(entry_lines(entry));
    }
    lines.push(format!("status {} documents", statuses.len()));
    for status in &statuses {
        lines.push(status_line(status));
    }

    Ok(lines.join("\n") + "\n")
}

fn spending(usage: Usage) -> String {
    let mut counted = Vec::new();
    for (label, count) in [
        ("input", usage.input),
        ("output", usage.output),
        ("total", usage.total),
        ("cached input", usage.cached_input),
        ("reasoning", usage.reasoning),
    ] {
        if let Some(count) = count {
            counted.push(format!("{label} {count}"));
        }
    }
    match counted.is_empty() {
        true => String::from("usage none reported"),
        false => format!("usage {}", counted.join(", ")),
    }
}

fn entry_lines(entry: &Entry) -> Vec<String> {
    match entry {
        Entry::Turn { turn } => vec![format!("  turn {turn}")],
        Entry::Call {
            turn,
            name,
            arguments,
        } => under(
            format!("  call {name} in turn {turn}"),
            &arguments.to_string(),
        ),
        Entry::Result {
            turn,
            name,
            text,
            bytes,
            elapsed_ms,
            exit,
        } => under(
            format!(
                "  result {name} in turn {turn}, {bytes} bytes in {elapsed_ms} ms, {}",
                verdict(*exit)
            ),
            &text.to_string(),
        ),
        Entry::Reasoning { turn, text } => {
            under(format!("  reasoning in turn {turn}"), &text.to_string())
        }
        Entry::Assistant { turn, text } => {
            under(format!("  assistant in turn {turn}"), &text.to_string())
        }
        Entry::Prompt { text } => under(String::from("  prompt"), &text.to_string()),
    }
}

fn verdict(exit: Option<Exit>) -> String {
    match exit {
        Some(exit) => exit.word(),
        None => String::from("no verdict recorded"),
    }
}

fn under(header: String, body: &str) -> Vec<String> {
    let mut lines = vec![header];
    for line in body.lines() {
        lines.push(format!("    {line}"));
    }
    lines
}

fn status_line(status: &Status) -> String {
    format!(
        "  {} {} turn {}: {}",
        status.phase().word(),
        status.workspace(),
        status.turn(),
        status.message()
    )
}

/// The files' own bytes, never decoded into this build's types and written back: a field
/// this build does not know is still the file's to state. A half line from a run killed
/// mid-write is left out, because half a line would leave the object unreadable.
fn own_shape(recorded: &Recorded) -> Result<String, String> {
    let directory = recorded.directory();
    let document = written(&document_path(directory))?;
    let transcript = lines(&transcript_path(directory))?;
    let statuses = lines(&status_path(directory))?;

    let mut object = String::from("{\"session\":");
    object.push_str(document.trim_end());
    object.push_str(",\n\"transcript\":[");
    object.push_str(&transcript.join(","));
    object.push_str("],\n\"status\":[");
    object.push_str(&statuses.join(","));
    object.push_str("]}\n");
    Ok(object)
}

fn written(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn lines(path: &Path) -> Result<Vec<String>, String> {
    read_lines(path).map_err(|error| error.to_string())
}
