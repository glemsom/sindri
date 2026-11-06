//! Configuration is one file, read and written whole and patched in place, so
//! two runs writing at once cannot lose each other's keys.
//!
//! Every key is an `Option`, because *unset* is a real state: it is what lets an
//! emptied value hand the key back to the environment, and what lets a layer
//! above stay silent instead of erasing what is below it.
//!
//! Keys layer file, then `SINDRI_*` environment, then flags. The environment is
//! read *above* the file, so an exported key wins; a flag is read above both,
//! and a repeated flag replaces the configured value rather than adding to it.
//!
//! The file is `$XDG_CONFIG_HOME/sindri/config.toml`, and it is the only one: a
//! `.sindri.toml` in the working directory would let a repository grant itself
//! writable paths, so nothing here can even ask for one.

use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;

use crate::wire::Wire;

/// What `Debug` prints in place of a secret, so a value cannot leak through a
/// diagnostic, a record or a verbose line (ADR-0011).
const MASK: &str = "«redacted»";

/// The one config schema and the one resolved config: every key optional, so
/// "unset" survives all the way down to whoever decides the default.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The wire dialect a run speaks.
    pub provider: Option<Wire>,
    /// The model a run asks for, named rather than invented.
    pub model: Option<String>,
    /// Where the wire's endpoint is, when it is not the dialect's own.
    pub base_url: Option<String>,
    /// The credential, which is a secret and is never printed.
    pub api_key: Option<String>,
    /// Whether the model reasons before it answers.
    pub thinking: Option<bool>,
    /// Paths added to the run's writable set, beyond the workspace.
    pub extra_writable_paths: Option<Vec<PathBuf>>,
    /// Request headers as `Name: value`, with `{session}` expanded per session.
    pub extra_headers: Option<Vec<String>>,
    /// The turn cap: a stop reason when it is reached, never an error.
    pub max_turns: Option<u32>,
    /// The per-call timeout in seconds.
    pub timeout: Option<u64>,
    /// The run's command wall clock, in milliseconds.
    pub command_budget_ms: Option<u64>,
    /// The run's budget of command output, in bytes.
    pub output_budget_bytes: Option<u64>,
    /// Whether OSC 7501 status reporting is on.
    pub status: Option<bool>,
    /// Whether the run's commands may use the user's ssh agent.
    pub ssh_agent: Option<bool>,
}

impl Config {
    /// Every key the schema has, in the order the settings modal walks them.
    pub const KEYS: [&'static str; 13] = [
        "provider",
        "model",
        "base_url",
        "api_key",
        "thinking",
        "extra_writable_paths",
        "extra_headers",
        "max_turns",
        "timeout",
        "command_budget_ms",
        "output_budget_bytes",
        "status",
        "ssh_agent",
    ];

    /// Reads the one file at `path`. An absent file is not an error: a first run
    /// needs no file at all, so every key simply stays unset.
    pub fn load_from(path: &Path) -> Result<Config, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default());
            }
            Err(error) => {
                return Err(ConfigError::Unreadable {
                    path: path.to_path_buf(),
                    reason: error.to_string(),
                });
            }
        };

        let table: toml::Table = toml::from_str(&text).map_err(|error| ConfigError::BadValue {
            origin: path.display().to_string(),
            reason: error.to_string(),
        })?;

        // `deny_unknown_fields` catches an unknown key too, but it reports the
        // serde shape of the schema rather than the key and the file the user
        // has to go and fix. Naming the key first is the whole point of §11.
        if let Some(key) = table
            .keys()
            .find(|key| !Config::KEYS.contains(&key.as_str()))
        {
            return Err(ConfigError::UnknownKey {
                key: key.clone(),
                path: path.to_path_buf(),
            });
        }

        table
            .try_into()
            .map_err(|error: toml::de::Error| ConfigError::BadValue {
                origin: path.display().to_string(),
                reason: error.to_string(),
            })
    }

    /// Reads the file the environment points at.
    pub fn load(environment: &impl Environment) -> Result<Config, ConfigError> {
        Config::load_from(&config_path(environment)?)
    }

    /// This layer, with every key it leaves unset taken from the layer below it.
    pub fn above(self, lower: Config) -> Config {
        Config {
            provider: self.provider.or(lower.provider),
            model: self.model.or(lower.model),
            base_url: self.base_url.or(lower.base_url),
            api_key: self.api_key.or(lower.api_key),
            thinking: self.thinking.or(lower.thinking),
            extra_writable_paths: self.extra_writable_paths.or(lower.extra_writable_paths),
            extra_headers: self.extra_headers.or(lower.extra_headers),
            max_turns: self.max_turns.or(lower.max_turns),
            timeout: self.timeout.or(lower.timeout),
            command_budget_ms: self.command_budget_ms.or(lower.command_budget_ms),
            output_budget_bytes: self.output_budget_bytes.or(lower.output_budget_bytes),
            status: self.status.or(lower.status),
            ssh_agent: self.ssh_agent.or(lower.ssh_agent),
        }
    }

    /// The environment's own layer: the `SINDRI_*` variables that speak.
    ///
    /// An exported nothing is silence rather than a value, so `SINDRI_MODEL=`
    /// leaves the file speaking instead of blanking the key.
    pub fn from_environment(environment: &impl Environment) -> Result<Config, ConfigError> {
        Ok(Config {
            provider: spoken_wire(environment, "SINDRI_PROVIDER")?,
            model: spoken(environment, "SINDRI_MODEL"),
            base_url: spoken(environment, "SINDRI_BASE_URL"),
            api_key: spoken(environment, "SINDRI_API_KEY"),
            thinking: spoken_parsed(environment, "SINDRI_THINKING")?,
            extra_writable_paths: spoken(environment, "SINDRI_EXTRA_WRITABLE_PATHS")
                .map(|paths| env::split_paths(&paths).collect()),
            extra_headers: spoken_headers(environment, "SINDRI_EXTRA_HEADERS")?,
            max_turns: spoken_parsed(environment, "SINDRI_MAX_TURNS")?,
            timeout: spoken_parsed(environment, "SINDRI_TIMEOUT")?,
            command_budget_ms: spoken_parsed(environment, "SINDRI_COMMAND_BUDGET_MS")?,
            output_budget_bytes: spoken_parsed(environment, "SINDRI_OUTPUT_BUDGET_BYTES")?,
            status: spoken_parsed(environment, "SINDRI_STATUS")?,
            ssh_agent: spoken_parsed(environment, "SINDRI_SSH_AGENT")?,
        })
    }

    /// The whole precedence in one call: file, then environment, then flags.
    pub fn resolve(environment: &impl Environment, flags: Flags) -> Result<Config, ConfigError> {
        let file = Config::load(environment)?;
        let environment = Config::from_environment(environment)?;
        Ok(Config::from(flags).above(environment.above(file)))
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| MASK))
            .field("thinking", &self.thinking)
            .field("extra_writable_paths", &self.extra_writable_paths)
            .field("extra_headers", &self.extra_headers.as_ref().map(|_| MASK))
            .field("max_turns", &self.max_turns)
            .field("timeout", &self.timeout)
            .field("command_budget_ms", &self.command_budget_ms)
            .field("output_budget_bytes", &self.output_budget_bytes)
            .field("status", &self.status)
            .field("ssh_agent", &self.ssh_agent)
            .finish()
    }
}

/// The flag layer as plain data: what a front end parsed, which the core layers
/// without ever learning what a command line is.
///
/// A flag that is given replaces the configured value rather than adding to it,
/// which is why every list here is a whole list and not an append.
#[derive(Clone, Default)]
pub struct Flags {
    /// The wire dialect a run speaks.
    pub provider: Option<Wire>,
    /// The model a run asks for, named rather than invented.
    pub model: Option<String>,
    /// Where the wire's endpoint is, when it is not the dialect's own.
    pub base_url: Option<String>,
    /// The credential, which is a secret and is never printed.
    pub api_key: Option<String>,
    /// Whether the model reasons before it answers.
    pub thinking: Option<bool>,
    /// Paths added to the run's writable set, beyond the workspace.
    pub extra_writable_paths: Option<Vec<PathBuf>>,
    /// Request headers as `Name: value`, with `{session}` expanded per session.
    pub extra_headers: Option<Vec<String>>,
    /// The turn cap: a stop reason when it is reached, never an error.
    pub max_turns: Option<u32>,
    /// The per-call timeout in seconds.
    pub timeout: Option<u64>,
    /// The run's command wall clock, in milliseconds.
    pub command_budget_ms: Option<u64>,
    /// The run's budget of command output, in bytes.
    pub output_budget_bytes: Option<u64>,
    /// Whether OSC 7501 status reporting is on.
    pub status: Option<bool>,
    /// Whether the run's commands may use the user's ssh agent.
    pub ssh_agent: Option<bool>,
}

impl From<Flags> for Config {
    fn from(flags: Flags) -> Config {
        Config {
            provider: flags.provider,
            model: flags.model,
            base_url: flags.base_url,
            api_key: flags.api_key,
            thinking: flags.thinking,
            extra_writable_paths: flags.extra_writable_paths,
            extra_headers: flags.extra_headers,
            max_turns: flags.max_turns,
            timeout: flags.timeout,
            command_budget_ms: flags.command_budget_ms,
            output_budget_bytes: flags.output_budget_bytes,
            status: flags.status,
            ssh_agent: flags.ssh_agent,
        }
    }
}

impl fmt::Debug for Flags {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Flags")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| MASK))
            .field("thinking", &self.thinking)
            .field("extra_writable_paths", &self.extra_writable_paths)
            .field("extra_headers", &self.extra_headers.as_ref().map(|_| MASK))
            .field("max_turns", &self.max_turns)
            .field("timeout", &self.timeout)
            .field("command_budget_ms", &self.command_budget_ms)
            .field("output_budget_bytes", &self.output_budget_bytes)
            .field("status", &self.status)
            .field("ssh_agent", &self.ssh_agent)
            .finish()
    }
}

/// The variables a config read may consult, so a test can hold them as data
/// rather than mutating the environment every other test in the process shares.
pub trait Environment {
    /// The value of `name`, or `None` when the variable is not exported.
    fn var(&self, name: &str) -> Option<String>;
}

impl Environment for BTreeMap<String, String> {
    fn var(&self, name: &str) -> Option<String> {
        self.get(name).cloned()
    }
}

/// The process's own environment, which is the only real one.
pub struct Process;

impl Environment for Process {
    fn var(&self, name: &str) -> Option<String> {
        env::var(name).ok()
    }
}

/// `$XDG_CONFIG_HOME/sindri/config.toml`, falling back to `$HOME/.config`.
pub fn config_path(environment: &impl Environment) -> Result<PathBuf, ConfigError> {
    let base = match spoken(environment, "XDG_CONFIG_HOME") {
        Some(home) => PathBuf::from(home),
        None => match spoken(environment, "HOME") {
            Some(home) => PathBuf::from(home).join(".config"),
            None => return Err(ConfigError::NoConfigDir),
        },
    };
    Ok(base.join("sindri").join("config.toml"))
}

/// A variable's value, with an exported nothing read as silence rather than as
/// a value that would blank the key.
fn spoken(environment: &impl Environment, name: &str) -> Option<String> {
    environment.var(name).filter(|value| !value.is_empty())
}

/// A variable read as some other type, naming the variable when it will not parse.
fn spoken_parsed<T>(environment: &impl Environment, name: &str) -> Result<Option<T>, ConfigError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    match spoken(environment, name) {
        None => Ok(None),
        Some(value) => value
            .parse::<T>()
            .map(Some)
            .map_err(|error| ConfigError::BadValue {
                origin: name.to_owned(),
                reason: format!("`{value}` will not parse: {error}"),
            }),
    }
}

/// A variable read as a wire, which accepts exactly the three words.
fn spoken_wire(environment: &impl Environment, name: &str) -> Result<Option<Wire>, ConfigError> {
    match spoken(environment, name) {
        None => Ok(None),
        Some(word) => Wire::from_word(&word)
            .map(Some)
            .ok_or_else(|| ConfigError::BadValue {
                origin: name.to_owned(),
                reason: format!(
                    "`{word}` is not a wire; expected one of {}",
                    Wire::WORDS.join(", ")
                ),
            }),
    }
}

/// A variable read as headers, one `Name: value` per line: a header value cannot
/// hold a newline, so a newline is the one separator that cannot be ambiguous.
fn spoken_headers(
    environment: &impl Environment,
    name: &str,
) -> Result<Option<Vec<String>>, ConfigError> {
    let Some(value) = spoken(environment, name) else {
        return Ok(None);
    };
    let headers: Vec<String> = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    if let Some(bad) = headers.iter().find(|header| !header.contains(':')) {
        return Err(ConfigError::BadValue {
            origin: name.to_owned(),
            reason: format!("`{bad}` is not a header; a header is `Name: value`"),
        });
    }
    Ok(Some(headers))
}

/// Why a config would not load. Every one of them is a usage error, so the
/// process exits 2 having named the key and the file it could not use.
#[derive(Debug)]
pub enum ConfigError {
    /// A key the schema does not have, in the one file that holds them.
    UnknownKey {
        /// The key as the file spelled it.
        key: String,
        /// The file it was spelled in.
        path: PathBuf,
    },
    /// A value that will not parse, named where it was found.
    BadValue {
        /// The file or the variable the value came from.
        origin: String,
        /// What was wrong with it.
        reason: String,
    },
    /// A file that could not be read at all.
    Unreadable {
        /// The file that would not open.
        path: PathBuf,
        /// Why it would not.
        reason: String,
    },
    /// Neither `XDG_CONFIG_HOME` nor `HOME` is set, so there is nowhere to look.
    NoConfigDir,
}

impl ConfigError {
    /// The exit code every config failure carries: a usage error, which is 2.
    pub fn exit_code(&self) -> u8 {
        2
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::UnknownKey { key, path } => {
                write!(formatter, "unknown key `{key}` in {}", path.display())
            }
            ConfigError::BadValue { origin, reason } => {
                write!(formatter, "bad value in {origin}: {reason}")
            }
            ConfigError::Unreadable { path, reason } => {
                write!(formatter, "cannot read {}: {reason}", path.display())
            }
            ConfigError::NoConfigDir => write!(
                formatter,
                "neither XDG_CONFIG_HOME nor HOME is set, so there is no config file to read"
            ),
        }
    }
}

impl Error for ConfigError {}
