//! The headers every provider request carries: where they come from, and what
//! the wire would refuse. One file, because a header's precedence and its
//! validity are one decision, and the config is where both are made.
//!
//! A table is judged in the order a save reads it: every line split into a name
//! and a value, and then every pair's name and value checked. [`refusal`] is
//! that check, and it answers with the one line it stops on, so a pane drawing
//! that line while it is typed and a save writing the table are one check and
//! cannot disagree about what a header is or which line the refusal is about.
//!
//! `{session}` in a value is checked here as the text it is: it is a value's
//! business, not a key's, and it becomes the run's session id where the request
//! is built, since no config can know which session reads it
//! (`crate::provider::SESSION_TOKEN`).

use std::collections::BTreeMap;

use reqwest::header::{HeaderName, HeaderValue};

use super::ConfigError;
use super::env;

/// Headers the wire writes itself. Sending one by hand would contradict the
/// request's own body, so it is refused when the config is read rather than
/// left to fail as a mystery 400 at the first turn.
const WIRE_HEADERS: [&str; 2] = ["content-length", "content-type"];

/// The headers that win: a flag, then the environment, then the file. As with
/// the extras, the highest level that names any replaces the rest, so a
/// pipeline sends exactly the headers it means rather than inheriting a file's.
pub(super) fn resolve(
    flag: Option<Vec<String>>,
    from_file: Option<BTreeMap<String, String>>,
) -> Result<Vec<(String, String)>, ConfigError> {
    if let Some(lines) = flag {
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        return header_pairs(&pairs_from_lines(&lines)?);
    }
    if let Some(variable) = env("SINDRI_EXTRA_HEADERS") {
        let lines: Vec<&str> = held(&variable).into_iter().map(|(_, line)| line).collect();
        return header_pairs(&pairs_from_lines(&lines)?);
    }
    match from_file {
        Some(named) => header_pairs(&named.into_iter().collect::<Vec<_>>()),
        None => Ok(Vec::new()),
    }
}

/// The line a table is refused for, and the sentence it earns: the check a save
/// runs, asked of a table a reader holds so a screen can draw that line in the
/// refusal's ink while it is typed, and clear it the keystroke the line is
/// legal. `table` is the table as the row holds it — `Name: value` one per line,
/// the blank lines a save drops included — and the line answered with is its
/// place among those lines, which is the line a screen draws it on.
///
/// The sentence is the one a save states, and `None` is a table a save takes
/// whole. A save splits every line before it judges any name or value, so a line
/// no `Name: value` could be is refused for the whole table, and it stops at the
/// first line it refuses rather than naming the ones below.
pub fn refusal(table: &str) -> Option<(usize, String)> {
    let held = held(table);
    let mut pairs: Vec<(usize, (String, String))> = Vec::with_capacity(held.len());
    for (entry, (at, text)) in held.iter().enumerate() {
        match split(text, entry) {
            Ok(pair) => pairs.push((*at, pair)),
            Err(error) => return Some((*at, error.message().to_string())),
        }
    }
    let mut judged: Vec<(String, String)> = Vec::with_capacity(pairs.len());
    for (at, (name, value)) in &pairs {
        match header(name, value, &judged) {
            Ok(pair) => judged.push(pair),
            Err(error) => return Some((*at, error.message().to_string())),
        }
    }
    None
}

/// The lines a save reads, each with the line of the table it stands on: the
/// table trimmed and its blank lines dropped, because a blank line is the place
/// the next header is typed and no entry a sentence can name. One rule, so the
/// place a sentence gives a line and the line a screen draws it on are counted
/// off the same text.
pub(super) fn held(table: &str) -> Vec<(usize, &str)> {
    table
        .split('\n')
        .enumerate()
        .map(|(at, text)| (at, text.trim()))
        .filter(|(_, text)| !text.is_empty())
        .collect()
}

/// `Name: value` lines, as a shell and a flag write them. A line with no colon
/// is refused without echoing it: whatever it is, it may be the header's
/// secret, and a usage error is not the place to print one.
pub(super) fn pairs_from_lines(lines: &[&str]) -> Result<Vec<(String, String)>, ConfigError> {
    lines
        .iter()
        .enumerate()
        .map(|(at, line)| split(line, at))
        .collect()
}

/// One line split into the name and the value it holds, neither of them read as
/// more than text: a line with no colon is no header at all, and the sentence
/// names the entry's place — `entry`, counted from nothing — rather than the
/// line it cannot print.
fn split(line: &str, entry: usize) -> Result<(String, String), ConfigError> {
    line.split_once(':').map_or_else(
        || {
            Err(ConfigError::Usage(format!(
                "extra_headers: entry {} is not `Name: value`",
                entry + 1
            )))
        },
        |(name, value)| Ok((name.to_string(), value.to_string())),
    )
}

/// The headers, checked before the run: a name or value the wire would refuse
/// is a usage error (exit 2) rather than a 400 at the first turn.
pub(super) fn header_pairs(
    given: &[(String, String)],
) -> Result<Vec<(String, String)>, ConfigError> {
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(given.len());
    for (name, value) in given {
        let pair = header(name, value, &pairs)?;
        pairs.push(pair);
    }
    pairs.sort();
    Ok(pairs)
}

/// One header, checked: every sentence a table can be refused with is written
/// here and nowhere else. Names are lowercased because HTTP names are
/// case-insensitive, and two spellings of one name are a duplicate, not two
/// headers — which is what `seen`, the headers judged before this one, is for.
fn header(
    name: &str,
    value: &str,
    seen: &[(String, String)],
) -> Result<(String, String), ConfigError> {
    let name = name.trim().to_ascii_lowercase();
    if HeaderName::from_bytes(name.as_bytes()).is_err() {
        return Err(ConfigError::Usage(format!(
            "extra_headers: `{name}` is not an HTTP header name"
        )));
    }
    if WIRE_HEADERS.contains(&name.as_str()) {
        return Err(ConfigError::Usage(format!(
            "extra_headers: `{name}` belongs to the request the wire writes; it cannot be set by hand"
        )));
    }
    if seen.iter().any(|(set, _)| *set == name) {
        return Err(ConfigError::Usage(format!(
            "extra_headers: `{name}` is set twice"
        )));
    }
    let trimmed = value.trim();
    HeaderValue::from_str(trimmed).map_err(|_| {
        ConfigError::Usage(format!(
            "extra_headers: the value of `{name}` is not a header value (printable text, no newline)"
        ))
    })?;
    Ok((name, trimmed.to_string()))
}
