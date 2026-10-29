//! The headers every provider request carries: where they come from, and what
//! the wire would refuse. One file, because a header's precedence and its
//! validity are one decision, and the config is where both are made.

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
        return header_pairs(&pairs_from_lines(&lines)?);
    }
    if let Some(variable) = env("SINDRI_EXTRA_HEADERS") {
        let lines: Vec<String> = variable
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect();
        return header_pairs(&pairs_from_lines(&lines)?);
    }
    match from_file {
        Some(named) => header_pairs(&named.into_iter().collect::<Vec<_>>()),
        None => Ok(Vec::new()),
    }
}

/// `Name: value` lines, as a shell and a flag write them. A line with no colon
/// is refused without echoing it: whatever it is, it may be the header's
/// secret, and a usage error is not the place to print one.
pub(super) fn pairs_from_lines(lines: &[String]) -> Result<Vec<(String, String)>, ConfigError> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            line.split_once(':').map_or_else(
                || {
                    Err(ConfigError::Usage(format!(
                        "extra_headers: entry {} is not `Name: value`",
                        index + 1
                    )))
                },
                |(name, value)| Ok((name.to_string(), value.to_string())),
            )
        })
        .collect()
}

/// The headers, checked before the run: a name or value the wire would refuse
/// is a usage error (exit 2) rather than a 400 at the first turn. Names are
/// lowercased because HTTP names are case-insensitive, and two spellings of one
/// name are a duplicate, not two headers.
pub(super) fn header_pairs(
    given: &[(String, String)],
) -> Result<Vec<(String, String)>, ConfigError> {
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(given.len());
    for (name, value) in given {
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
        if pairs.iter().any(|(seen, _)| *seen == name) {
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
        pairs.push((name, trimmed.to_string()));
    }
    pairs.sort();
    Ok(pairs)
}
