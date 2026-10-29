//! The config file as something a form can edit: one key at a time, checked
//! against the schema the run itself reads (ADR-0005), and written only when
//! the user asks for it (ADR-0014). Nothing here is a second schema: a value
//! this refuses is a value the run would refuse.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::file::{self, FileConfig};
use super::{ConfigError, MAX_TIMEOUT_SECS, Provider, extra_paths};

/// One key of the config file, named the way the file names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// The wire to speak.
    Provider,
    /// The model to ask.
    Model,
    /// Another API root.
    BaseUrl,
    /// The provider credential: a secret, never handed back.
    ApiKey,
    /// The turn cap.
    MaxTurns,
    /// How long one command may run.
    Timeout,
    /// Whether the run pays for reasoning.
    Thinking,
    /// Extra paths granted read-write inside the sandbox.
    ExtraWritablePaths,
    /// Headers every provider request carries: secret values, never shown.
    ExtraHeaders,
}

/// Every key, in the order a form shows them: what a run is changed for first,
/// and what is rarely touched last.
pub const KEYS: [Key; 9] = [
    Key::Provider,
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
    /// The key's own name in the file, which is also what an error calls it.
    pub const fn name(self) -> &'static str {
        match self {
            Key::Provider => "provider",
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
    /// reader, so no front end can print one by accident (ADR-0005, ADR-0014).
    pub const fn secret(self) -> bool {
        matches!(self, Key::ApiKey | Key::ExtraHeaders)
    }
}

/// One config file, staged for editing. Reading it writes nothing; only
/// [`Document::save`] touches the disk.
#[derive(Debug, Clone)]
pub struct Document {
    path: PathBuf,
    values: FileConfig,
}

impl Document {
    /// The config file a run reads.
    pub fn load() -> Result<Document, ConfigError> {
        Document::load_at(super::file_path())
    }

    /// The config file at `path`: a test points one at a temporary directory,
    /// because no test writes a real user's config.
    pub fn load_at(path: impl Into<PathBuf>) -> Result<Document, ConfigError> {
        let path = path.into();
        let values = file::read_at(&path)?;
        Ok(Document { path, values })
    }

    /// Where saving would write.
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

    /// Whether the file names this key at all: what a form says instead of a
    /// secret's value.
    pub fn is_set(&self, key: Key) -> bool {
        match key {
            Key::Provider => self.values.provider.is_some(),
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

    /// One key, checked and staged. An empty value unsets the key, so the
    /// environment, a flag or the default speaks instead. A value the schema
    /// refuses is an error and changes nothing.
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

/// A count the schema accepts, said the way the run says it.
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

/// A toggle, the way the environment takes one: a typo is refused rather than
/// silently reading as off.
fn toggle(value: &str) -> Result<bool, ConfigError> {
    match value.trim() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(ConfigError::Usage(format!(
            "is not a toggle: `{other}` (use true or false)"
        ))),
    }
}

/// The extras, `:`-separated the way the environment writes them, checked by
/// the writable set's own rules so a path the sandbox would refuse is refused
/// here first.
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
