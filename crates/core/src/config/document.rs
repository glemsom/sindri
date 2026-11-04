//! The config file as something a form can edit: one key at a time, checked
//! against the schema the run itself reads, and written only when
//! the user asks for it. Nothing here is a second schema: a value
//! this refuses is a value the run would refuse.
//!
//! One table holds every key whole, and it is the only list of them: `KEYS`,
//! the name, the secret flag and the three things a document does with a key
//! all answer from a row, so a key is added in one place or not at all.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::file::{self, FileConfig};
use super::{ConfigError, MAX_TURNS, Provider, extra_paths};
use crate::exec::MAX_TIMEOUT_SECS;
use crate::paths;
use crate::sandbox::Extras;

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Provider,
    Model,
    BaseUrl,
    ApiKey,
    MaxTurns,
    Timeout,
    Thinking,
    Status,
    SshAgent,
    ExtraWritablePaths,
    ExtraHeaders,
}

/// How many keys there are: the table's length, and the modal's.
const N_KEYS: usize = 11;

pub const KEYS: [Key; N_KEYS] = keys();

/// The keys the form draws, in the order the table holds them, so the modal's
/// rows are the table's rows and not a second list that could lose one.
const fn keys() -> [Key; N_KEYS] {
    let mut keys = [SPECS[0].key; N_KEYS];
    let mut index = 0;
    while index < N_KEYS {
        keys[index] = SPECS[index].key;
        index += 1;
    }
    keys
}

/// One key as the form sees it: what it is called, whether its value is a
/// secret or a toggle, what its field starts with, and how the file carries it.
struct Spec {
    key: Key,
    name: &'static str,
    /// A secret is never handed back to the reader, so no front end can print
    /// one by accident. Its `get` says only that the file holds one.
    secret: bool,
    /// A toggle is a key whose value is one of two words, so the form offers
    /// `Space` on it and says which of them the field means: the row and the
    /// keypress read the one flag rather than a second list of the toggles.
    toggled: bool,
    /// A key of lines is a value that is a list rather than one word — the
    /// headers, one `Name: value` per line — so the row is as tall as the list
    /// it holds and a paste of the clipboard's lines is the list and not one
    /// run of them.
    lines: bool,
    /// The text a field starts with, whatever the file says: the run's own
    /// default, which is nothing for every key but the writable paths — a key
    /// whose default is a value the form must show rather than one it may
    /// leave blank.
    start: fn() -> String,
    /// Whether an empty field is a value the key carries rather than the form's
    /// way of unsetting it. Only a list has one: `[]` and "not set" are two
    /// sentences about the writable paths, and the second one is the shipped
    /// floor.
    empty_is_a_value: bool,
    get: fn(&FileConfig) -> Option<String>,
    clear: fn(&mut FileConfig),
    put: fn(&mut FileConfig, &str) -> Result<(), ConfigError>,
}

/// Every key, one row each, read by the key's own number: one row per variant,
/// at that variant's number, or the crate does not build. The four things a
/// document does with a key and the row the modal draws it from are then one
/// row rather than four matches.
const SPECS: [Spec; N_KEYS] = [
    Spec {
        key: Key::Provider,
        name: "provider",
        secret: false,
        toggled: false,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.provider.clone(),
        clear: |values| values.provider = None,
        put: |values, value| {
            values.provider = Some(Provider::parse(value)?.as_str().to_string());
            Ok(())
        },
    },
    Spec {
        key: Key::Model,
        name: "model",
        secret: false,
        toggled: false,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.model.clone(),
        clear: |values| values.model = None,
        put: |values, value| {
            values.model = Some(value.to_string());
            Ok(())
        },
    },
    Spec {
        key: Key::BaseUrl,
        name: "base_url",
        secret: false,
        toggled: false,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.base_url.clone(),
        clear: |values| values.base_url = None,
        put: |values, value| {
            values.base_url = Some(value.to_string());
            Ok(())
        },
    },
    Spec {
        key: Key::ApiKey,
        name: "api_key",
        secret: true,
        toggled: false,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.api_key.as_ref().map(|_| String::new()),
        clear: |values| values.api_key = None,
        put: |values, value| {
            values.api_key = Some(value.to_string());
            Ok(())
        },
    },
    Spec {
        key: Key::MaxTurns,
        name: "max_turns",
        secret: false,
        toggled: false,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.max_turns.map(|turns| turns.to_string()),
        clear: |values| values.max_turns = None,
        put: |values, value| {
            values.max_turns = Some(count(value, 1, MAX_TURNS)?);
            Ok(())
        },
    },
    Spec {
        key: Key::Timeout,
        name: "timeout",
        secret: false,
        toggled: false,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.timeout.map(|seconds| seconds.to_string()),
        clear: |values| values.timeout = None,
        put: |values, value| {
            values.timeout = Some(count(value, 1, MAX_TIMEOUT_SECS)?);
            Ok(())
        },
    },
    Spec {
        key: Key::Thinking,
        name: "thinking",
        secret: false,
        toggled: true,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.thinking.map(|on| on.to_string()),
        clear: |values| values.thinking = None,
        put: |values, value| {
            values.thinking = Some(toggle(value)?);
            Ok(())
        },
    },
    Spec {
        key: Key::Status,
        name: "status",
        secret: false,
        toggled: true,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.status.map(|on| on.to_string()),
        clear: |values| values.status = None,
        put: |values, value| {
            values.status = Some(toggle(value)?);
            Ok(())
        },
    },
    Spec {
        key: Key::SshAgent,
        name: "ssh_agent",
        secret: false,
        toggled: true,
        lines: false,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| values.ssh_agent.map(|on| on.to_string()),
        clear: |values| values.ssh_agent = None,
        put: |values, value| {
            values.ssh_agent = Some(toggle(value)?);
            Ok(())
        },
    },
    Spec {
        key: Key::ExtraWritablePaths,
        name: "extra_writable_paths",
        secret: false,
        toggled: false,
        lines: false,
        // The row starts at the floor, so a blank row never stands for the
        // four mounts a run with nothing configured would really make.
        start: floor_text,
        empty_is_a_value: true,
        get: |values| {
            values
                .extra_writable_paths
                .as_ref()
                .map(|paths| paths.join(":"))
        },
        clear: |values| values.extra_writable_paths = None,
        put: |values, value| {
            values.extra_writable_paths = Some(
                paths(value)?
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect(),
            );
            Ok(())
        },
    },
    Spec {
        key: Key::ExtraHeaders,
        name: "extra_headers",
        // A header is not a credential: the row shows the table the file holds,
        // one `Name: value` per line, and the values are handed back like any
        // other row's — the file is where they live either way.
        secret: false,
        toggled: false,
        lines: true,
        start: || String::new(),
        empty_is_a_value: false,
        get: |values| {
            values
                .extra_headers
                .as_ref()
                .filter(|headers| !headers.is_empty())
                .map(|headers| {
                    headers
                        .iter()
                        .map(|(name, value)| format!("{name}: {value}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
        },
        clear: |values| values.extra_headers = None,
        put: |values, value| {
            values.extra_headers = Some(headers(value)?);
            Ok(())
        },
    },
];

/// A row read at the key's own number is that key's row only while the table
/// holds each key at its own number: `keys` and the modal's order are the
/// table's, so a row that moved would draw one key and write another's field,
/// and that is a build to refuse rather than a run to debug.
const _: () = {
    let mut index = 0;
    while index < N_KEYS {
        assert!(
            SPECS[index].key as usize == index,
            "one row per key, at the key's own number"
        );
        index += 1;
    }
};

impl Key {
    /// The key's own row, read at the key's own number: a lookup is an index,
    /// never a search that can come up short.
    fn spec(self) -> &'static Spec {
        &SPECS[self as usize]
    }

    pub fn name(self) -> &'static str {
        self.spec().name
    }

    /// Whether the value is a secret. A secret is never handed back to the
    /// reader, so no front end can print one by accident.
    pub fn secret(self) -> bool {
        self.spec().secret
    }

    /// Whether the value is one of two words, so `Space` flips it and the form
    /// says which of them the field means. The row is the only list of them.
    pub fn toggled(self) -> bool {
        self.spec().toggled
    }

    /// Whether the value is a list of lines rather than one word: the headers,
    /// one `Name: value` each. A row of lines is as tall as the list it holds,
    /// and a paste into it keeps the clipboard's breaks.
    pub fn lines(self) -> bool {
        self.spec().lines
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    path: PathBuf,
    values: FileConfig,
}

impl Document {
    pub fn load() -> Result<Document, ConfigError> {
        Document::load_at(super::file_path())
    }

    pub fn load_at(path: impl Into<PathBuf>) -> Result<Document, ConfigError> {
        let path = path.into();
        let values = file::read_at(&path)?;
        Ok(Document { path, values })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The text a form starts a field with. A secret starts empty whatever the
    /// file holds: the value is the file's, and a screen is not where it lives.
    pub fn editable(&self, key: Key) -> String {
        let spec = key.spec();
        if spec.secret {
            return String::new();
        }
        (spec.get)(&self.values).unwrap_or_else(spec.start)
    }

    pub fn is_set(&self, key: Key) -> bool {
        (key.spec().get)(&self.values).is_some()
    }

    pub fn set(&mut self, key: Key, value: &str) -> Result<(), ConfigError> {
        let value = value.trim();
        let spec = key.spec();
        if value.is_empty() && !spec.empty_is_a_value {
            self.unset(key);
            return Ok(());
        }
        (spec.put)(&mut self.values, value)
    }

    /// Writes the file, immediately and atomically. This is the only place the
    /// frame writes anything outside the session record.
    pub fn save(&self) -> Result<(), ConfigError> {
        file::write(&self.path, &self.values)
    }

    /// Clears the key: the file carries no value here, and a cleared key falls
    /// back to the run's default — the floor, for the writable paths.
    pub fn unset(&mut self, key: Key) {
        (key.spec().clear)(&mut self.values);
    }
}

/// The row's start: the shipped floor as this host has it. One derivation with
/// the policy's, so the row shows the mounts the run would really make and
/// never a path the sandbox would drop.
fn floor_text() -> String {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    Extras::Floor
        .resolve(paths::home().as_deref(), &workspace)
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

fn count<T>(value: &str, low: T, high: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialOrd + std::fmt::Display + Copy,
{
    // Two failures, two sentences: a count the type cannot hold is a number,
    // and calling it "not a number" sends the reader after a typo that is not
    // there. It names the bound the key takes, which is what to stay under.
    // A leading `+` is a sign the parse accepts, so it does not make the count
    // unreadable — `+5` and `5000000000` are counts, one of them too large.
    let parsed: T = value.parse().map_err(|_| {
        let digits = value.strip_prefix('+').unwrap_or(value);
        if !digits.is_empty() && digits.chars().all(|digit| digit.is_ascii_digit()) {
            ConfigError::Usage(format!(
                "too large: a count no larger than {high}, got {value}"
            ))
        } else {
            ConfigError::Usage(format!("not a number: `{value}`"))
        }
    })?;
    if parsed < low || parsed > high {
        return Err(ConfigError::Usage(format!(
            "must be between {low} and {high}, got {parsed}"
        )));
    }
    Ok(parsed)
}

pub fn toggle(value: &str) -> Result<bool, ConfigError> {
    match value.trim() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(ConfigError::Usage(format!(
            "is not a toggle: `{other}` (use true or false)"
        ))),
    }
}

/// What a form should put on screen for a toggle: a value the file does not
/// carry is the run's own default, which is off. One spelling, so a
/// field and a file cannot disagree about what on means.
pub fn toggle_shown(value: &str) -> &'static str {
    match toggle(value).unwrap_or(false) {
        true => "on",
        false => "off",
    }
}

fn paths(value: &str) -> Result<Vec<PathBuf>, ConfigError> {
    let items: Vec<String> = value
        .split(':')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect();
    extra_paths(&items)
}

/// Headers, as `Name: value` entries, one per line. A `;` is not a separator:
/// it is legal inside a value (`Cookie: a=1; b=2`), and a value the table in
/// the file carries whole is not cut in two here. An error names the entry's
/// position and never the line, which may still be one worth keeping quiet.
fn headers(value: &str) -> Result<BTreeMap<String, String>, ConfigError> {
    // The lines a save reads, which is the same reading a pane's refusal is
    // judged by: one rule for which lines a table holds and where each of them
    // stands, so a row and a file never disagree about the entry a sentence
    // names.
    let lines: Vec<&str> = super::headers::held(value)
        .into_iter()
        .map(|(_, line)| line)
        .collect();
    let pairs = super::headers::pairs_from_lines(&lines)?;
    Ok(super::headers::header_pairs(&pairs)?.into_iter().collect())
}
