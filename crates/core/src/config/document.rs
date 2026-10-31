//! The config file as something a form can edit: one key at a time, checked
//! against the schema the run itself reads, and written only when
//! the user asks for it. Nothing here is a second schema: a value
//! this refuses is a value the run would refuse.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::file::{self, FileConfig};
use super::{ConfigError, Provider, Theme, extra_paths};
use crate::exec::MAX_TIMEOUT_SECS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Provider,
    Theme,
    Model,
    BaseUrl,
    ApiKey,
    MaxTurns,
    Timeout,
    Thinking,
    ExtraWritablePaths,
    ExtraHeaders,
}

pub const KEYS: [Key; 10] = [
    Key::Provider,
    Key::Theme,
    Key::Model,
    Key::BaseUrl,
    Key::ApiKey,
    Key::MaxTurns,
    Key::Timeout,
    Key::Thinking,
    Key::ExtraWritablePaths,
    Key::ExtraHeaders,
];

impl Key {
    pub const fn name(self) -> &'static str {
        match self {
            Key::Provider => "provider",
            Key::Theme => "theme",
            Key::Model => "model",
            Key::BaseUrl => "base_url",
            Key::ApiKey => "api_key",
            Key::MaxTurns => "max_turns",
            Key::Timeout => "timeout",
            Key::Thinking => "thinking",
            Key::ExtraWritablePaths => "extra_writable_paths",
            Key::ExtraHeaders => "extra_headers",
        }
    }

    /// Whether the value is a secret. A secret is never handed back to the
    /// reader, so no front end can print one by accident.
    pub const fn secret(self) -> bool {
        matches!(self, Key::ApiKey | Key::ExtraHeaders)
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
        if key.secret() {
            return String::new();
        }
        match key {
            Key::Provider => self.values.provider.clone().unwrap_or_default(),
            Key::Theme => self
                .values
                .theme
                .clone()
                .unwrap_or_else(|| Theme::default().as_str().to_string()),
            Key::Model => self.values.model.clone().unwrap_or_default(),
            Key::BaseUrl => self.values.base_url.clone().unwrap_or_default(),
            Key::MaxTurns => self
                .values
                .max_turns
                .map(|n| n.to_string())
                .unwrap_or_default(),
            Key::Timeout => self
                .values
                .timeout
                .map(|n| n.to_string())
                .unwrap_or_default(),
            Key::Thinking => self
                .values
                .thinking
                .map(|on| on.to_string())
                .unwrap_or_default(),
            Key::ExtraWritablePaths => self
                .values
                .extra_writable_paths
                .as_ref()
                .map(|paths| paths.join(":"))
                .unwrap_or_default(),
            Key::ApiKey | Key::ExtraHeaders => String::new(),
        }
    }

    pub fn is_set(&self, key: Key) -> bool {
        match key {
            Key::Provider => self.values.provider.is_some(),
            Key::Theme => self.values.theme.is_some(),
            Key::Model => self.values.model.is_some(),
            Key::BaseUrl => self.values.base_url.is_some(),
            Key::ApiKey => self.values.api_key.is_some(),
            Key::MaxTurns => self.values.max_turns.is_some(),
            Key::Timeout => self.values.timeout.is_some(),
            Key::Thinking => self.values.thinking.is_some(),
            Key::ExtraWritablePaths => self.values.extra_writable_paths.is_some(),
            Key::ExtraHeaders => self
                .values
                .extra_headers
                .as_ref()
                .is_some_and(|headers| !headers.is_empty()),
        }
    }

    pub fn set(&mut self, key: Key, value: &str) -> Result<(), ConfigError> {
        let value = value.trim();
        if value.is_empty() {
            self.unset(key);
            return Ok(());
        }
        match key {
            Key::Provider => {
                self.values.provider = Some(Provider::parse(value)?.as_str().to_string());
            }
            Key::Theme => self.values.theme = Some(Theme::parse(value)?.as_str().to_string()),
            Key::Model => self.values.model = Some(value.to_string()),
            Key::BaseUrl => self.values.base_url = Some(value.to_string()),
            Key::ApiKey => self.values.api_key = Some(value.to_string()),
            Key::MaxTurns => self.values.max_turns = Some(count(value, 1, u32::MAX)?),
            Key::Timeout => {
                let seconds: u64 = count(value, 1, MAX_TIMEOUT_SECS)?;
                self.values.timeout = Some(seconds);
            }
            Key::Thinking => self.values.thinking = Some(toggle(value)?),
            Key::ExtraWritablePaths => {
                self.values.extra_writable_paths = Some(
                    paths(value)?
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect(),
                );
            }
            Key::ExtraHeaders => self.values.extra_headers = Some(headers(value)?),
        }
        Ok(())
    }

    /// Writes the file, immediately and atomically. This is the only place the
    /// frame writes anything outside the session record.
    pub fn save(&self) -> Result<(), ConfigError> {
        file::write(&self.path, &self.values)
    }

    fn unset(&mut self, key: Key) {
        match key {
            Key::Provider => self.values.provider = None,
            Key::Theme => self.values.theme = None,
            Key::Model => self.values.model = None,
            Key::BaseUrl => self.values.base_url = None,
            Key::ApiKey => self.values.api_key = None,
            Key::MaxTurns => self.values.max_turns = None,
            Key::Timeout => self.values.timeout = None,
            Key::Thinking => self.values.thinking = None,
            Key::ExtraWritablePaths => self.values.extra_writable_paths = None,
            Key::ExtraHeaders => self.values.extra_headers = None,
        }
    }
}

fn count<T>(value: &str, low: T, high: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialOrd + std::fmt::Display + Copy,
{
    let parsed: T = value
        .parse()
        .map_err(|_| ConfigError::Usage(format!("not a number: `{value}`")))?;
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

/// Headers, as `Name: value` entries separated by a newline or a `;`. The
/// values are secrets: an error names the position, never the text.
fn headers(value: &str) -> Result<BTreeMap<String, String>, ConfigError> {
    let lines: Vec<String> = value
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let pairs = super::headers::pairs_from_lines(&lines)?;
    Ok(super::headers::header_pairs(&pairs)?.into_iter().collect())
}
