//! A run's settings, and which source wins: the config file, then `SINDRI_*`
//! environment variables, then flags. One file, read from
//! `$XDG_CONFIG_HOME/sindri/config.toml` and nowhere else, so a cloned
//! repository can never configure the agent.
//!
//! Two kinds of failure leave by two doors, because a shell branches on them:
//! a value that is present but wrong is a usage error (exit 2), while a value
//! the run cannot do without is a model error (exit 1) with a hint naming
//! every way to set it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::exec::{DEFAULT_TIMEOUT_SECS, timeout_from_secs};
use crate::paths;
use crate::sandbox::{Enforcement, WritableSet};

pub const DEFAULT_MAX_TURNS: u32 = 250;

mod document;
mod file;
mod headers;
mod provider;
mod theme;

pub use document::{Document, KEYS, Key, toggle, toggle_shown};
pub use provider::Provider;
pub use theme::Theme;

#[derive(Debug, Default, Clone)]
pub struct Overrides {
    pub provider: Option<String>,
    pub theme: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub max_turns: Option<u32>,
    pub timeout: Option<u64>,
    pub thinking: Option<bool>,
    pub writable: Option<Vec<String>>,
    pub extra_headers: Option<Vec<String>>,
    /// Asked for in the invocation that makes it, and only there: a persisted
    /// unsafe setting is the one a user forgets they turned on.
    pub yolo_unsafe: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: Provider,
    pub theme: Theme,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub max_turns: u32,
    pub timeout: Duration,
    pub thinking: bool,
    pub extra_writable: Vec<PathBuf>,
    /// The policy the session starts with, and the one its record states.
    pub enforcement: Enforcement,
    /// Never printed or recorded: a gateway's header may be as secret as the
    /// credential.
    pub extra_headers: Vec<(String, String)>,
    /// A repository-local config that was found and refused, for the front end
    /// to note once. It is never read.
    pub refused_repo_config: Option<PathBuf>,
}

pub const REPO_CONFIG_NAME: &str = ".sindri.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// A value is present but wrong: exit 2.
    Usage(String),
    /// A value the run needs is absent: exit 1, with a hint.
    Missing(String),
}

impl ConfigError {
    pub fn message(&self) -> &str {
        match self {
            ConfigError::Usage(message) | ConfigError::Missing(message) => message,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Model {
    /// A run cannot start without one: exit 1, with a hint naming every way to
    /// set it.
    Required,
    /// The front end is a door before it is a run: `doctor`, `sandbox exec`
    /// and the frame's first screen all resolve a config that an absent model
    /// or credential cannot stop, because none of them sends a prompt yet
    Optional,
}

impl Config {
    pub fn resolve(overrides: Overrides) -> Result<Config, ConfigError> {
        Config::read(overrides, Model::Required)
    }

    /// The read for a front end that is a door before it is a run: `doctor`,
    /// `sandbox exec`, and the frame's first screen. Neither the model nor the
    /// credential is demanded — the frame's settings modal is where a missing
    /// one goes — and every value that *is* present is checked
    /// exactly as `resolve` checks it.
    pub fn for_front_end(overrides: Overrides) -> Result<Config, ConfigError> {
        Config::read(overrides, Model::Optional)
    }

    fn read(overrides: Overrides, asked: Model) -> Result<Config, ConfigError> {
        let file = file::read()?;

        let provider = match overrides
            .provider
            .or_else(|| env("SINDRI_PROVIDER"))
            .or(file.provider)
        {
            Some(name) => Provider::parse(&name)?,
            None => Provider::Anthropic,
        };

        // A theme is a name from a closed set, so an unknown one is a usage
        // error wherever it was named. The message lists the file beside the
        // flag and the variable, the way a missing value's hint does, so one
        // line fixes a typo made in any of the three.
        let theme = match overrides
            .theme
            .or_else(|| env("SINDRI_THEME"))
            .or(file.theme)
        {
            Some(name) => Theme::parse(&name).map_err(|error| {
                ConfigError::Usage(format!(
                    "{}, pass --theme, set SINDRI_THEME, or set `theme` in {}",
                    error.message(),
                    file_path()
                ))
            })?,
            None => Theme::Anvil,
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
        let timeout = timeout_from_secs(timeout_secs).map_err(ConfigError::Usage)?;

        let named = overrides
            .model
            .or_else(|| env("SINDRI_MODEL"))
            .or(file.model)
            .filter(|model| !model.trim().is_empty());
        let model = match named {
            Some(model) => model,
            None if asked == Model::Optional => String::new(),
            None => {
                return Err(ConfigError::Missing(format!(
                    "no model configured for the {} wire: pass --model, set SINDRI_MODEL, or set `model` in {}{}",
                    provider.as_str(),
                    file_path(),
                    provider.model_hint().unwrap_or_default()
                )));
            }
        };

        let api_key = env("SINDRI_API_KEY")
            .or_else(|| env(provider.api_key_env()))
            .or(file.api_key)
            .filter(|key| !key.trim().is_empty());
        let api_key = match api_key {
            Some(key) => key,
            None if !provider.needs_credential() || asked == Model::Optional => String::new(),
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
        let extra_headers = headers::resolve(overrides.extra_headers, file.extra_headers)?;

        let enforcement = if overrides.yolo_unsafe {
            Enforcement::YoloUnsafe
        } else {
            Enforcement::Bubblewrap
        };

        Ok(Config {
            refused_repo_config: repo_config(),
            provider,
            theme,
            model,
            api_key,
            base_url,
            max_turns,
            timeout,
            thinking,
            extra_writable,
            enforcement,
            extra_headers,
        })
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

/// The repository-local config sitting in the working directory, if any. Its
/// existence is all that is ever looked at: a checked-in file that grants extra
/// writable paths would turn every clone into a privilege escalation, so it is
/// refused rather than merged.
fn repo_config() -> Option<PathBuf> {
    let candidate = Path::new(REPO_CONFIG_NAME);
    candidate.is_file().then(|| candidate.to_path_buf())
}

fn file_path() -> String {
    paths::config_file()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "$XDG_CONFIG_HOME/sindri/config.toml".to_string())
}

/// One environment variable, with an empty value read as unset: an exported
/// variable left empty is how a shell un-sets one for a single command.
pub(super) fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

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
