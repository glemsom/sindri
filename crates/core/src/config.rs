//! A run's settings, and which source wins: the config file, then `SINDRI_*`
//! environment variables, then flags (ADR-0005). One file, read from
//! `$XDG_CONFIG_HOME/sindri/config.toml` and nowhere else, so a cloned
//! repository can never configure the agent.
//!
//! Two kinds of failure leave by two doors, because a shell branches on them:
//! a value that is present but wrong is a usage error (exit 2), while a value
//! the run cannot do without is a model error (exit 1) with a hint naming
//! every way to set it.
//!
//! The environment's keys mirror the file's: `SINDRI_PROVIDER`, `SINDRI_MODEL`,
//! `SINDRI_API_KEY`, `SINDRI_BASE_URL`, `SINDRI_MAX_TURNS`, `SINDRI_TIMEOUT`,
//! `SINDRI_THINKING`, `SINDRI_EXTRA_WRITABLE_PATHS`;
//! a provider's own `ANTHROPIC_API_KEY`/`ANTHROPIC_BASE_URL` or
//! `OPENAI_API_KEY`/`OPENAI_BASE_URL` are read below those, so a user with one
//! already exported needs no configuration at all.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::paths;
use crate::sandbox::WritableSet;

/// How many turns the run may take before it stops (the turn cap).
pub const DEFAULT_MAX_TURNS: u32 = 250;
/// The default per-command timeout the sandbox enforces.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
/// The longest a single command may be allowed to run.
pub const MAX_TIMEOUT_SECS: u64 = 3600;

mod provider;

pub use provider::Provider;

/// What the command line says, before anything else has a say.
#[derive(Debug, Default, Clone)]
pub struct Overrides {
    /// `--provider`.
    pub provider: Option<String>,
    /// `--model`.
    pub model: Option<String>,
    /// `--base-url`.
    pub base_url: Option<String>,
    /// `--max-turns`.
    pub max_turns: Option<u32>,
    /// `--timeout`, in seconds.
    pub timeout: Option<u64>,
    /// `--thinking` (`Some(true)`) or `--no-thinking` (`Some(false)`).
    pub thinking: Option<bool>,
    /// `--writable`, as written.
    pub writable: Option<Vec<String>>,
}

/// A run's settings, resolved and validated: nothing optional that the run
/// cannot do without.
#[derive(Debug, Clone)]
pub struct Config {
    /// The wire to speak.
    pub provider: Provider,
    /// The model to ask.
    pub model: String,
    /// The provider credential, never printed or recorded unredacted.
    pub api_key: String,
    /// Another API root: a gateway, a proxy, or a local server.
    pub base_url: Option<String>,
    /// The turn cap.
    pub max_turns: u32,
    /// How long one command may run inside the sandbox.
    pub timeout: Duration,
    /// Whether the run pays for the model's reasoning.
    pub thinking: bool,
    /// Extra paths granted read-write besides the workspace, checked to be
    /// mountable.
    pub extra_writable: Vec<PathBuf>,
    /// A repository-local config that was found and refused, for the front end
    /// to note once. It is never read (ADR-0005).
    pub refused_repo_config: Option<PathBuf>,
}

/// The name a config committed in a repository would have. It is looked for so
/// the refusal can be said out loud, and never opened.
pub const REPO_CONFIG_NAME: &str = ".sindri.toml";

/// Why a run could not be configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// A value is present but wrong: exit 2.
    Usage(String),
    /// A value the run needs is absent: exit 1, with a hint.
    Missing(String),
}

impl ConfigError {
    /// The one line the front end prints for this error.
    pub fn message(&self) -> &str {
        match self {
            ConfigError::Usage(message) | ConfigError::Missing(message) => message,
        }
    }
}

/// The config file's own shape: every key optional, every key a string or a
/// count. A key the file does not name is left unset; a key the schema does not
/// name is an error, because a typo'd `max_turn` would otherwise run on
/// defaults the user never chose.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    max_turns: Option<u32>,
    timeout: Option<u64>,
    thinking: Option<bool>,
    extra_writable_paths: Option<Vec<String>>,
}

impl Config {
    /// The settings this run uses: file, then environment, then flags, with
    /// every value checked.
    pub fn resolve(overrides: Overrides) -> Result<Config, ConfigError> {
        let file = read_file()?;

        let provider = match overrides
            .provider
            .or_else(|| env("SINDRI_PROVIDER"))
            .or(file.provider)
        {
            Some(name) => Provider::parse(&name)?,
            None => Provider::Anthropic,
        };

        let base_url = overrides
            .base_url
            .or_else(|| env("SINDRI_BASE_URL"))
            .or(file.base_url)
            .or_else(|| env(provider.base_url_env()))
            .filter(|url| !url.trim().is_empty());

        let max_turns = overrides
            .max_turns
            .or(number("SINDRI_MAX_TURNS")?)
            .or(file.max_turns)
            .unwrap_or(DEFAULT_MAX_TURNS);
        if max_turns == 0 {
            return Err(ConfigError::Usage(
                "max_turns must be at least 1".to_string(),
            ));
        }

        let timeout_secs = overrides
            .timeout
            .or(number("SINDRI_TIMEOUT")?.map(u64::from))
            .or(file.timeout)
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        if timeout_secs == 0 || timeout_secs > MAX_TIMEOUT_SECS {
            return Err(ConfigError::Usage(format!(
                "timeout must be between 1 and {MAX_TIMEOUT_SECS} seconds, got {timeout_secs}"
            )));
        }

        let model = overrides
            .model
            .or_else(|| env("SINDRI_MODEL"))
            .or(file.model)
            .filter(|model| !model.trim().is_empty())
            .ok_or_else(|| {
                ConfigError::Missing(format!(
                    "no model configured for the {} wire: pass --model, set SINDRI_MODEL, or set `model` in {}{}",
                    provider.as_str(),
                    file_path(),
                    provider.model_hint().unwrap_or_default()
                ))
            })?;

        let api_key = env("SINDRI_API_KEY")
            .or_else(|| env(provider.api_key_env()))
            .or(file.api_key)
            .filter(|key| !key.trim().is_empty());
        // A wire that needs no credential runs without one; a wire that does
        // says so, naming every way to set it.
        let api_key = match api_key {
            Some(key) => key,
            None if !provider.needs_credential() => String::new(),
            None => {
                return Err(ConfigError::Missing(format!(
                    "no credential for {}: set {} or SINDRI_API_KEY, or set `api_key` in {}",
                    provider.as_str(),
                    provider.api_key_env(),
                    file_path()
                )));
            }
        };

        let thinking = overrides
            .thinking
            .or(boolean("SINDRI_THINKING")?)
            .or(file.thinking)
            .unwrap_or(false);

        let extra_writable = extras(overrides.writable, file.extra_writable_paths)?;

        Ok(Config {
            refused_repo_config: repo_config(),
            provider,
            model,
            api_key,
            base_url,
            max_turns,
            timeout: Duration::from_secs(timeout_secs),
            thinking,
            extra_writable,
        })
    }
}

/// Reads the config file. An absent file is an empty one; an unreadable or
/// unparsable file is a usage error, never silently ignored.
fn read_file() -> Result<FileConfig, ConfigError> {
    let path = file_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|error| ConfigError::Usage(parse_error(&path, &text, &error))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileConfig::default()),
        Err(error) => Err(ConfigError::Usage(format!("{path}: {error}"))),
    }
}

impl Config {
    /// The extras a run would be granted, from the file and the environment
    /// alone: `sindri doctor` needs them to describe the sandbox a run really
    /// gets, and must not need a model or a credential to find out (ADR-0002).
    pub fn configured_extras(overrides: Overrides) -> Result<Vec<PathBuf>, ConfigError> {
        let file = read_file()?;
        extras(overrides.writable, file.extra_writable_paths)
    }
}

/// The extras that win: a flag, then the environment, then the file. The
/// highest level that names any replaces the rest rather than adding to them,
/// so a pipeline can grant exactly what it means.
fn extras(
    flag: Option<Vec<String>>,
    from_file: Option<Vec<String>>,
) -> Result<Vec<PathBuf>, ConfigError> {
    flag.or_else(|| list("SINDRI_EXTRA_WRITABLE_PATHS"))
        .or(from_file)
        .map(|paths| extra_paths(&paths))
        .transpose()
        .map(Option::unwrap_or_default)
}

/// A parse error, without the source snippet TOML would print: that snippet is
/// the file's own text, and the credential may well be on that line. The
/// message and the position are kept, because those are what a user acts on.
fn parse_error(path: &str, text: &str, error: &toml::de::Error) -> String {
    let Some(span) = error.span() else {
        return format!("{path}: {}", error.message());
    };
    let before = &text[..span.start.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, str::len) + 1;
    format!(
        "{path}: {} at line {line}, column {column}",
        error.message()
    )
}

/// The repository-local config sitting in the working directory, if any. Its
/// existence is all that is ever looked at: a checked-in file that grants extra
/// writable paths would turn every clone into a privilege escalation, so it is
/// refused rather than merged.
fn repo_config() -> Option<PathBuf> {
    let candidate = Path::new(REPO_CONFIG_NAME);
    candidate.is_file().then(|| candidate.to_path_buf())
}

/// Where the config file would be, for hints.
fn file_path() -> String {
    paths::config_file()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "$XDG_CONFIG_HOME/sindri/config.toml".to_string())
}

/// One environment variable, with an empty value read as unset: an exported
/// variable left empty is how a shell un-sets one for a single command.
fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// A list from the environment, `:`-separated the way `PATH` is. Present but
/// empty is an unset variable: an exported but empty list grants nothing.
fn list(name: &str) -> Option<Vec<String>> {
    env(name).map(|value| {
        value
            .split(':')
            .filter(|item| !item.trim().is_empty())
            .map(str::to_string)
            .collect()
    })
}

/// The extras, checked before the sandbox is built so a bad path is a value
/// error (exit 2) rather than a mystery setup failure. The rules are the
/// writable set's own — absolute, canonical, not `/`, no duplicate, no nesting —
/// asked once, here, so the two can never disagree.
fn extra_paths(given: &[String]) -> Result<Vec<PathBuf>, ConfigError> {
    WritableSet::new(given.iter().map(PathBuf::from))
        .map(|set| set.paths().to_vec())
        .map_err(|error| ConfigError::Usage(format!("extra_writable_paths: {error}")))
}

/// A toggle from the environment. Present but neither `true` nor `false` is a
/// usage error, so a typo cannot silently mean "off".
fn boolean(name: &str) -> Result<Option<bool>, ConfigError> {
    let Some(value) = env(name) else {
        return Ok(None);
    };
    match value.trim() {
        "true" | "1" | "yes" | "on" => Ok(Some(true)),
        "false" | "0" | "no" | "off" => Ok(Some(false)),
        other => Err(ConfigError::Usage(format!(
            "{name} is not a toggle: `{other}` (use true or false)"
        ))),
    }
}

/// A count from the environment. Present but unparsable is a usage error: a
/// typo must not silently fall back to the default.
fn number(name: &str) -> Result<Option<u32>, ConfigError> {
    let Some(value) = env(name) else {
        return Ok(None);
    };
    value
        .trim()
        .parse::<u32>()
        .map(Some)
        .map_err(|_| ConfigError::Usage(format!("{name} is not a number: `{value}`")))
}
