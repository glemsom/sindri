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
use crate::sandbox::{Enforcement, Extras, WritableSet};

pub const DEFAULT_MAX_TURNS: u32 = 250;

/// The largest turn cap a run takes, and the bound the form states: a cap
/// nobody could reach is not a setting, and a refusal has to name a number the
/// reader can act on rather than the width of the type it was parsed into.
pub const MAX_TURNS: u32 = 10_000;

mod document;
mod file;
mod headers;
mod provider;

pub use document::{Document, KEYS, Key, toggle, toggle_shown};
pub use headers::refusal;
pub use provider::Provider;

#[derive(Debug, Default, Clone)]
pub struct Overrides {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub max_turns: Option<u32>,
    pub timeout: Option<u64>,
    pub thinking: Option<bool>,
    /// Whether the sandbox is given the host's ssh agent socket. `--ssh-agent`
    /// and `--no-ssh-agent` are the two ends of the one key.
    pub ssh_agent: Option<bool>,
    pub writable: Option<Vec<String>>,
    pub extra_headers: Option<Vec<String>>,
    /// Asked for in the invocation that makes it, and only there: a persisted
    /// unsafe setting is the one a user forgets they turned on.
    pub yolo_unsafe: bool,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: Provider,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub max_turns: u32,
    pub timeout: Duration,
    pub thinking: bool,
    /// Whether a run tells its terminal what it is doing: on unless the file or
    /// the environment silences it, because one switch has to reach both front
    /// ends and a per-front-end flag would be two.
    pub status: bool,
    /// Whether the sandbox is handed the host's ssh agent socket, so `git push`
    /// and `ssh-add -l` work in it: on unless the file or the environment says
    /// off, because an assistant that cannot use the keys it was given cannot
    /// finish the job.
    pub ssh_agent: bool,
    /// The writable paths besides the workspace and the session temp: the
    /// shipped floor where no level names a list, and exactly what was named
    /// where one does — an empty list included, which means none.
    pub extras: Extras,
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
        if !(1..=MAX_TURNS).contains(&max_turns) {
            return Err(ConfigError::Usage(format!(
                "max_turns must be between 1 and {MAX_TURNS}, got {max_turns}"
            )));
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

        let api_key = if provider == Provider::Copilot {
            None
        } else {
            env("SINDRI_API_KEY")
                .or_else(|| env(provider.api_key_env()))
                .or(file.api_key)
                .filter(|key| !key.trim().is_empty())
        };
        let api_key = match api_key {
            Some(key) => key,
            None if provider == Provider::Copilot => {
                if asked == Model::Required {
                    return Err(ConfigError::Missing(
                        "GitHub Copilot sign-in required: run `sindri auth login copilot`"
                            .to_string(),
                    ));
                }
                String::new()
            }
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

        let status = boolean("SINDRI_STATUS")?.or(file.status).unwrap_or(true);

        let ssh_agent = overrides
            .ssh_agent
            .or(boolean("SINDRI_SSH_AGENT")?)
            .or(file.ssh_agent)
            .unwrap_or(true);

        let extras = extras(overrides.writable, file.extra_writable_paths)?;
        let extra_headers = headers::resolve(overrides.extra_headers, file.extra_headers)?;

        let enforcement = if overrides.yolo_unsafe {
            Enforcement::YoloUnsafe
        } else {
            Enforcement::Bubblewrap
        };

        Ok(Config {
            refused_repo_config: repo_config(),
            provider,
            model,
            api_key,
            base_url,
            max_turns,
            timeout,
            thinking,
            status,
            ssh_agent,
            extras,
            enforcement,
            extra_headers,
        })
    }
}

/// The extras that win: a flag, then the environment, then the file. The
/// highest level that names any replaces the rest rather than adding to them,
/// so a pipeline can grant exactly what it means — and a level that names none
/// means the shipped floor, which is the one thing no level has to say.
fn extras(
    flag: Option<Vec<String>>,
    from_file: Option<Vec<String>>,
) -> Result<Extras, ConfigError> {
    match flag
        .or_else(|| list("SINDRI_EXTRA_WRITABLE_PATHS"))
        .or(from_file)
    {
        Some(paths) => extra_paths(&paths).map(Extras::Named),
        None => Ok(Extras::Floor),
    }
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

/// A toggle from the environment, read by the file's own list of the words that
/// mean on and off: a spelling one source takes the other takes, and a refusal
/// is the file's words behind the variable's name.
fn boolean(name: &str) -> Result<Option<bool>, ConfigError> {
    let Some(value) = env(name) else {
        return Ok(None);
    };
    toggle(&value)
        .map(Some)
        .map_err(|error| ConfigError::Usage(format!("{name}: {}", error.message())))
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
