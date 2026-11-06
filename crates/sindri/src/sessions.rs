//! `sindri sessions` reads past runs back: `list` names every session the state
//! home holds, newest first, and `show` reads one back from a whole id or an
//! unambiguous prefix, with `--json` handing the files on in their own shape so
//! `jq` and the transcript cannot disagree (§13).
//!
//! Nothing here decides what a session is, which session an id names, what order
//! a listing is in, or what an id that names none of them costs: the core reads
//! the directories, chooses the session and gives the refusal its §15 exit code,
//! and this module writes the bytes and says nothing else. A listing is stdout's
//! alone and a refusal is stderr's alone, so a script that reads a listing never
//! has to filter it.
//!
//! Reading a past run needs no config, no key and no network, which is why the
//! command is answered before a first run is resolved: a person looking up what a
//! killed run did is not asked to sign in first.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use sindri_core::config::Process;
use sindri_core::records::{
    Entry, Recorded, Status, Usage, choose, document_path, read_lines, read_sessions, read_status,
    read_transcript, status_path, transcript_path,
};
use sindri_core::session;

use crate::cli::SessionsAction;
use crate::refuse;

/// §15's setup failure: a state home that cannot be read, or a session's files
/// that cannot be.
const SETUP: u8 = 3;

/// §15's usage error: an id that names no one session, or a `show` with no run
/// behind it to read.
const USAGE: u8 = 2;

/// Runs `sindri sessions …`: it reads what the state home holds and writes what
/// was asked for, and it never runs anything.
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

/// One line per session, newest first: the id to type, when the run started, why
/// it stopped, how far it got, and what it asked — which is what a person
/// choosing one to read back needs and no more.
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

/// One session read back: the id first, so a reader knows what to type next, then
/// what the run was, where it stood, when it started, why it stopped — `running`
/// until a stop reason lands, which a session `/new` superseded keeps saying,
/// because it did not stop, it was left (§13) — and then the transcript and the
/// status documents in the order they were written.
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

/// The session's facts and both of its line files, as the one text a reader is
/// shown: the document says what the run was, the transcript what happened, and
/// the status documents what it said it was doing, and nothing here restates any
/// of them.
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
    // A run that never stopped is given no end it did not reach, so the word the
    // document carries is the whole answer for a session that was left.
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

/// What the run's asks spent, only the counts a wire actually reported and in the
/// document's own field order: a count nobody reported is left out rather than
/// written as a zero (§15).
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

/// One entry of the transcript as a reader is shown it: the fact it is, and the
/// text it carries under it, one step further in — never a line the run stored.
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
        } => under(
            format!("  result {name} in turn {turn}, {bytes} bytes in {elapsed_ms} ms"),
            &text.to_string(),
        ),
        Entry::Assistant { turn, text } => {
            under(format!("  assistant in turn {turn}"), &text.to_string())
        }
    }
}

/// A header and the words under it, every line of them indented one step
/// further: what a call asked, what a result said and what the assistant
/// answered are the model's own text and keep their line breaks.
fn under(header: String, body: &str) -> Vec<String> {
    let mut lines = vec![header];
    for line in body.lines() {
        lines.push(format!("    {line}"));
    }
    lines
}

/// One status document as a reader is shown it: the state, the workspace's name,
/// the turn and the run's own words (§14).
fn status_line(status: &Status) -> String {
    format!(
        "  {} {} turn {}: {}",
        status.phase().word(),
        status.workspace(),
        status.turn(),
        status.message()
    )
}

/// The session's three files in their own shape: `session.json` as the run left
/// it under `session`, and the lines of `transcript.jsonl` and `status.json` under
/// `transcript` and `status` — the files' own bytes, so `jq` and the transcript
/// cannot disagree (§13).
///
/// Nothing here decodes an entry into this build's types and writes it back out:
/// a field this build does not know, and a kind of entry it has no variant for,
/// are still the file's to state, and a reader that re-spoke them would drop what
/// it could not read. A line that is not JSON at all — half of one, from a run
/// killed mid-write — is the one thing left out, because a half line is no shape
/// at all and would leave the object unreadable.
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

/// The session document, whole, or the failure to read it named: the run's own
/// words are handed on as it wrote them.
fn written(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// The lines of a line file, each one kept as the file spells it and only the
/// ones that are a JSON value: the core reads them, so a line is read here by the
/// same rule the transcript is read by.
fn lines(path: &Path) -> Result<Vec<String>, String> {
    read_lines(path).map_err(|error| error.to_string())
}
