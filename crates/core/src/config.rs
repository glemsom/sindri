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
//! `SINDRI_API_KEY`, `SINDRI_BASE_URL`, `SINDRI_MAX_TURNS`, `SINDRI_TIMEOUT`;
//! a provider's own `ANTHROPIC_API_KEY` and `ANTHROPIC_BASE_URL` are read below
//! those, so a user with a key already exported needs no configuration at all.

use std::time::Duration;

use serde::Deserialize;

use crate::paths;

/// How many turns the run may take before it stops (the turn cap).
pub const DEFAULT_MAX_TURNS: u32 = 250;
/// The default per-command timeout the sandbox enforces.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
/// The longest a single command may be allowed to run.
pub const MAX_TIMEOUT_SECS: u64 = 3600;

/// The wire a run speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Anthropic's Messages API. The OpenAI-compatible wire is a later slice.
    Anthropic,
}

impl Provider {
    /// The provider a name selects.
    pub fn parse(name: &str) -> Result<Self, ConfigError> {
        match name {
            "anthropic" => Ok(Provider::Anthropic),
            other => Err(ConfigError::Usage(format!(
                "unknown provider `{other}`: the only provider so far is `anthropic`"
            ))),
        }
    }

    /// The name this provider is configured and recorded as.
    pub const fn as_str(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
        }
    }

    /// The variable this provider's credential normally lives in.
    pub const fn api_key_env(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_API_KEY",
        }
    }

    /// The variable this provider's own client would read its base URL from.
    pub const fn base_url_env(self) -> &'static str {
        match self {
            Provider::Anthropic => "ANTHROPIC_BASE_URL",
        }
    }
}

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
}

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
/// count, and nothing else. A key the file does not name is left unset.
#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    provider: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    max_turns: Option<u32>,
    timeout: Option<u64>,
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
                    "no model configured: pass --model, set SINDRI_MODEL, or set `model` in {}",
                    file_path()
                ))
            })?;

        let api_key = env("SINDRI_API_KEY")
            .or_else(|| env(provider.api_key_env()))
            .or(file.api_key)
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| {
                ConfigError::Missing(format!(
                    "no credential for {}: set {} or SINDRI_API_KEY, or set `api_key` in {}",
                    provider.as_str(),
                    provider.api_key_env(),
                    file_path()
                ))
            })?;

        Ok(Config {
            provider,
            model,
            api_key,
            base_url,
            max_turns,
            timeout: Duration::from_secs(timeout_secs),
        })
    }
}

/// Reads the config file. An absent file is an empty one; an unreadable or
/// unparsable file is a usage error, never silently ignored.
fn read_file() -> Result<FileConfig, ConfigError> {
    let path = file_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|error| ConfigError::Usage(format!("{path}: {error}")))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileConfig::default()),
        Err(error) => Err(ConfigError::Usage(format!("{path}: {error}"))),
    }
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
