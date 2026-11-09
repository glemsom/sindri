use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use serde::Deserialize;
use toml_edit::{DocumentMut, Item, Value};
use crate::provider::Route;
use crate::redact::{MASK, SecretSet};
use crate::wire::Wire;
pub const MODE: u32 = 0o600;
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub provider: Option<Wire>,
    pub model: Option<String>,
    pub route: Option<Route>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub thinking: Option<bool>,
    pub extra_writable_paths: Option<Vec<PathBuf>>,
    pub extra_headers: Option<Vec<String>>,
    pub context_window: Option<u64>,
    pub max_turns: Option<u32>,
    pub timeout: Option<u64>,
    pub command_budget_ms: Option<u64>,
    pub output_budget_bytes: Option<u64>,
    pub status: Option<bool>,
    pub ssh_agent: Option<bool>,
}
impl Config {
    pub const KEYS: [&'static str; 15] = [
        "provider",
        "model",
        "route",
        "base_url",
        "api_key",
        "thinking",
        "extra_writable_paths",
        "extra_headers",
        "context_window",
        "max_turns",
        "timeout",
        "command_budget_ms",
        "output_budget_bytes",
        "status",
        "ssh_agent",
    ];
    pub const TOGGLES: [&'static str; 3] = ["thinking", "status", "ssh_agent"];
    pub fn tells_status(&self) -> bool {
        self.status.unwrap_or(true)
    }
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
        if let Some(key) = table
            .keys()
            .find(|key| !Config::KEYS.contains(&key.as_str()))
        {
            return Err(ConfigError::UnknownKey {
                key: key.clone(),
                path: path.to_path_buf(),
            });
        }
        let config: Config =
            table
                .try_into()
                .map_err(|error: toml::de::Error| ConfigError::BadValue {
                    origin: path.display().to_string(),
                    reason: error.to_string(),
                })?;
        if let Some(window) = config.context_window {
            nonzero_window(&path.display().to_string(), window)?;
        }
        Ok(config)
    }
    pub fn load(environment: &impl Environment) -> Result<Config, ConfigError> {
        Config::load_from(&config_path(environment)?)
    }
    pub fn save_to(path: &Path, touched: &Flags) -> Result<(), ConfigError> {
        Config::repatch(path, |document| touched.patch_into(document))
    }
    pub fn save_touched_to(path: &Path, touched: &Touched) -> Result<(), ConfigError> {
        let (flags, emptied) = touched.patch()?;
        Config::repatch(path, |document| {
            for key in &emptied {
                document.remove(key);
            }
            flags.patch_into(document)
        })
    }
    fn repatch(
        path: &Path,
        patch: impl FnOnce(&mut DocumentMut) -> Result<(), ConfigError>,
    ) -> Result<(), ConfigError> {
        let existing = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return Err(ConfigError::Unreadable {
                    path: path.to_path_buf(),
                    reason: error.to_string(),
                });
            }
        };
        let mut document =
            existing
                .parse::<DocumentMut>()
                .map_err(|error| ConfigError::BadValue {
                    origin: path.display().to_string(),
                    reason: error.to_string(),
                })?;
        patch(&mut document)?;
        write_atomically(path, &document.to_string())
    }
    pub fn save(environment: &impl Environment, touched: &Flags) -> Result<(), ConfigError> {
        Config::save_to(&config_path(environment)?, touched)
    }
    pub fn save_touched(
        environment: &impl Environment,
        touched: &Touched,
    ) -> Result<(), ConfigError> {
        Config::save_touched_to(&config_path(environment)?, touched)
    }
    pub fn text(&self, key: &str) -> String {
        let shown = |value: &Option<String>| value.clone().unwrap_or_default();
        match key {
            "provider" => self
                .provider
                .map(|wire| wire.word().to_owned())
                .unwrap_or_default(),
            "model" => shown(&self.model),
            "route" => self
                .route
                .map(|route| route.word().to_owned())
                .unwrap_or_default(),
            "base_url" => shown(&self.base_url),
            "api_key" => self
                .api_key
                .as_ref()
                .map(|_| MASK.to_owned())
                .unwrap_or_default(),
            "thinking" => on_or_empty(self.thinking),
            "extra_writable_paths" => self
                .extra_writable_paths
                .as_ref()
                .map(|paths| {
                    paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<String>>()
                        .join(LIST_SEPARATOR)
                })
                .unwrap_or_default(),
            "extra_headers" => self
                .extra_headers
                .as_ref()
                .map(|headers| headers.join(LIST_SEPARATOR))
                .unwrap_or_default(),
            "context_window" => number_or_empty(self.context_window),
            "max_turns" => number_or_empty(self.max_turns),
            "timeout" => number_or_empty(self.timeout),
            "command_budget_ms" => number_or_empty(self.command_budget_ms),
            "output_budget_bytes" => number_or_empty(self.output_budget_bytes),
            "status" => on_or_empty(self.status),
            "ssh_agent" => on_or_empty(self.ssh_agent),
            _ => String::new(),
        }
    }
    pub fn above(self, lower: Config) -> Config {
        Config {
            provider: self.provider.or(lower.provider),
            model: self.model.or(lower.model),
            route: self.route.or(lower.route),
            base_url: self.base_url.or(lower.base_url),
            api_key: self.api_key.or(lower.api_key),
            thinking: self.thinking.or(lower.thinking),
            extra_writable_paths: self.extra_writable_paths.or(lower.extra_writable_paths),
            extra_headers: self.extra_headers.or(lower.extra_headers),
            context_window: self.context_window.or(lower.context_window),
            max_turns: self.max_turns.or(lower.max_turns),
            timeout: self.timeout.or(lower.timeout),
            command_budget_ms: self.command_budget_ms.or(lower.command_budget_ms),
            output_budget_bytes: self.output_budget_bytes.or(lower.output_budget_bytes),
            status: self.status.or(lower.status),
            ssh_agent: self.ssh_agent.or(lower.ssh_agent),
        }
    }
    pub fn from_environment(environment: &impl Environment) -> Result<Config, ConfigError> {
        Ok(Config {
            provider: spoken_wire(environment, "SINDRI_PROVIDER")?,
            model: spoken(environment, "SINDRI_MODEL"),
            route: spoken_route(environment, "SINDRI_ROUTE")?,
            base_url: spoken(environment, "SINDRI_BASE_URL"),
            api_key: spoken(environment, "SINDRI_API_KEY"),
            thinking: spoken_parsed(environment, "SINDRI_THINKING")?,
            extra_writable_paths: spoken(environment, "SINDRI_EXTRA_WRITABLE_PATHS")
                .map(|paths| env::split_paths(&paths).collect()),
            extra_headers: spoken_headers(environment, "SINDRI_EXTRA_HEADERS")?,
            context_window: spoken_window(environment, "SINDRI_CONTEXT_WINDOW")?,
            max_turns: spoken_parsed(environment, "SINDRI_MAX_TURNS")?,
            timeout: spoken_parsed(environment, "SINDRI_TIMEOUT")?,
            command_budget_ms: spoken_parsed(environment, "SINDRI_COMMAND_BUDGET_MS")?,
            output_budget_bytes: spoken_parsed(environment, "SINDRI_OUTPUT_BUDGET_BYTES")?,
            status: spoken_parsed(environment, "SINDRI_STATUS")?,
            ssh_agent: spoken_parsed(environment, "SINDRI_SSH_AGENT")?,
        })
    }
    pub fn secrets(&self) -> SecretSet {
        let mut secrets = SecretSet::default();
        for secret in self.api_key.iter() {
            secrets.add(secret.as_str());
        }
        for header in self.extra_headers.iter().flatten() {
            secrets.add(header.as_str());
            if let Some((_, value)) = header.split_once(':') {
                secrets.add(value.trim());
            }
        }
        secrets
    }
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
            .field("route", &self.route)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| MASK))
            .field("thinking", &self.thinking)
            .field("extra_writable_paths", &self.extra_writable_paths)
            .field("extra_headers", &self.extra_headers.as_ref().map(|_| MASK))
            .field("context_window", &self.context_window)
            .field("max_turns", &self.max_turns)
            .field("timeout", &self.timeout)
            .field("command_budget_ms", &self.command_budget_ms)
            .field("output_budget_bytes", &self.output_budget_bytes)
            .field("status", &self.status)
            .field("ssh_agent", &self.ssh_agent)
            .finish()
    }
}
#[derive(Clone, Default)]
pub struct Flags {
    pub provider: Option<Wire>,
    pub model: Option<String>,
    pub route: Option<Route>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub thinking: Option<bool>,
    pub extra_writable_paths: Option<Vec<PathBuf>>,
    pub extra_headers: Option<Vec<String>>,
    pub context_window: Option<u64>,
    pub max_turns: Option<u32>,
    pub timeout: Option<u64>,
    pub command_budget_ms: Option<u64>,
    pub output_budget_bytes: Option<u64>,
    pub status: Option<bool>,
    pub ssh_agent: Option<bool>,
}
impl Flags {
    fn patch_into(&self, document: &mut DocumentMut) -> Result<(), ConfigError> {
        if let Some(wire) = self.provider {
            write_key(document, "provider", Value::from(wire.word()));
        }
        if let Some(value) = &self.model {
            write_string(document, "model", value);
        }
        if let Some(route) = self.route {
            write_key(document, "route", Value::from(route.word()));
        }
        if let Some(value) = &self.base_url {
            write_string(document, "base_url", value);
        }
        if let Some(value) = &self.api_key {
            write_string(document, "api_key", value);
        }
        if let Some(value) = self.thinking {
            write_key(document, "thinking", Value::from(value));
        }
        if let Some(paths) = &self.extra_writable_paths {
            let strings: Vec<String> = paths
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            write_key(document, "extra_writable_paths", array(strings));
        }
        if let Some(headers) = &self.extra_headers {
            write_key(document, "extra_headers", array(headers.clone()));
        }
        if let Some(tokens) = self.context_window {
            write_key(
                document,
                "context_window",
                integer("context_window", nonzero_window("context_window", tokens)?)?,
            );
        }
        if let Some(turns) = self.max_turns {
            write_key(document, "max_turns", Value::from(i64::from(turns)));
        }
        if let Some(seconds) = self.timeout {
            write_key(document, "timeout", integer("timeout", seconds)?);
        }
        if let Some(milliseconds) = self.command_budget_ms {
            write_key(
                document,
                "command_budget_ms",
                integer("command_budget_ms", milliseconds)?,
            );
        }
        if let Some(bytes) = self.output_budget_bytes {
            write_key(
                document,
                "output_budget_bytes",
                integer("output_budget_bytes", bytes)?,
            );
        }
        if let Some(value) = self.status {
            write_key(document, "status", Value::from(value));
        }
        if let Some(value) = self.ssh_agent {
            write_key(document, "ssh_agent", Value::from(value));
        }
        Ok(())
    }
}
impl From<Flags> for Config {
    fn from(flags: Flags) -> Config {
        Config {
            provider: flags.provider,
            model: flags.model,
            route: flags.route,
            base_url: flags.base_url,
            api_key: flags.api_key,
            thinking: flags.thinking,
            extra_writable_paths: flags.extra_writable_paths,
            extra_headers: flags.extra_headers,
            context_window: flags.context_window,
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
            .field("route", &self.route)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| MASK))
            .field("thinking", &self.thinking)
            .field("extra_writable_paths", &self.extra_writable_paths)
            .field("extra_headers", &self.extra_headers.as_ref().map(|_| MASK))
            .field("context_window", &self.context_window)
            .field("max_turns", &self.max_turns)
            .field("timeout", &self.timeout)
            .field("command_budget_ms", &self.command_budget_ms)
            .field("output_budget_bytes", &self.output_budget_bytes)
            .field("status", &self.status)
            .field("ssh_agent", &self.ssh_agent)
            .finish()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Touched {
    keys: BTreeMap<&'static str, String>,
}
const LIST_SEPARATOR: &str = ", ";
impl Touched {
    pub fn new() -> Touched {
        Touched::default()
    }
    pub fn set(&mut self, key: &'static str, text: impl Into<String>) {
        self.keys.insert(key, text.into());
    }
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
    pub fn rows(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.keys.iter().map(|(key, text)| (*key, text.as_str()))
    }
    fn patch(&self) -> Result<(Flags, Vec<&'static str>), ConfigError> {
        let mut flags = Flags::default();
        let mut emptied: Vec<&'static str> = Vec::new();
        for (key, held) in &self.keys {
            let text = held.trim();
            if text.is_empty() {
                emptied.push(key);
                continue;
            }
            match *key {
                "provider" => flags.provider = Some(as_wire(key, text)?),
                "model" => flags.model = Some(text.to_owned()),
                "route" => flags.route = Some(as_route(key, text)?),
                "base_url" => flags.base_url = Some(text.to_owned()),
                "api_key" => flags.api_key = Some(text.to_owned()),
                "thinking" => flags.thinking = Some(parsed(key, text)?),
                "extra_writable_paths" => {
                    flags.extra_writable_paths =
                        Some(listed(text).into_iter().map(PathBuf::from).collect());
                }
                "extra_headers" => flags.extra_headers = Some(headers_of(key, text)?),
                "context_window" => flags.context_window = Some(parsed(key, text)?),
                "max_turns" => flags.max_turns = Some(parsed(key, text)?),
                "timeout" => flags.timeout = Some(parsed(key, text)?),
                "command_budget_ms" => flags.command_budget_ms = Some(parsed(key, text)?),
                "output_budget_bytes" => flags.output_budget_bytes = Some(parsed(key, text)?),
                "status" => flags.status = Some(parsed(key, text)?),
                "ssh_agent" => flags.ssh_agent = Some(parsed(key, text)?),
                _ => {
                    return Err(ConfigError::BadValue {
                        origin: (*key).to_owned(),
                        reason: String::from("not a key of the config file"),
                    });
                }
            }
        }
        Ok((flags, emptied))
    }
}
fn listed(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}
fn headers_of(key: &str, text: &str) -> Result<Vec<String>, ConfigError> {
    let headers = listed(text);
    header_shape(key, &headers)?;
    Ok(headers)
}
fn on_or_empty(value: Option<bool>) -> String {
    value.map(|on| on.to_string()).unwrap_or_default()
}
fn number_or_empty(number: Option<impl fmt::Display>) -> String {
    number.map(|value| value.to_string()).unwrap_or_default()
}
fn write_key(document: &mut DocumentMut, key: &str, value: Value) {
    let mut item = Item::Value(value);
    match document.get_mut(key) {
        Some(existing) => {
            if let (Some(old), Some(fresh)) = (existing.as_value(), item.as_value_mut()) {
                *fresh.decor_mut() = old.decor().clone();
            }
            *existing = item;
        }
        None => {
            document.insert(key, item);
        }
    }
}
fn write_string(document: &mut DocumentMut, key: &str, value: &str) {
    if value.is_empty() {
        document.remove(key);
    } else {
        write_key(document, key, Value::from(value));
    }
}
fn array(values: Vec<String>) -> Value {
    let mut array = toml_edit::Array::new();
    for value in values {
        array.push(value);
    }
    Value::Array(array)
}
fn integer(key: &str, value: u64) -> Result<Value, ConfigError> {
    i64::try_from(value)
        .map(Value::from)
        .map_err(|_| ConfigError::BadValue {
            origin: key.to_owned(),
            reason: format!("{value} is larger than a config file's integer"),
        })
}
fn write_atomically(path: &Path, text: &str) -> Result<(), ConfigError> {
    let unwritable = |reason: String| ConfigError::Unwritable {
        path: path.to_path_buf(),
        reason,
    };
    let directory = match path.parent() {
        Some(directory) if !directory.as_os_str().is_empty() => directory,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(directory).map_err(|error| unwritable(error.to_string()))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("config.toml"));
    let mut staged = tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".staged")
        .tempfile_in(directory)
        .map_err(|error| unwritable(error.to_string()))?;
    staged
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(MODE))
        .map_err(|error| unwritable(error.to_string()))?;
    staged
        .write_all(text.as_bytes())
        .map_err(|error| unwritable(error.to_string()))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|error| unwritable(error.to_string()))?;
    staged
        .persist(path)
        .map_err(|error| unwritable(error.error.to_string()))?;
    let _ = std::fs::File::open(directory).and_then(|directory| directory.sync_all());
    Ok(())
}
pub trait Environment {
    fn var(&self, name: &str) -> Option<String>;
}
impl Environment for BTreeMap<String, String> {
    fn var(&self, name: &str) -> Option<String> {
        self.get(name).cloned()
    }
}
pub struct Process;
impl Environment for Process {
    fn var(&self, name: &str) -> Option<String> {
        env::var(name).ok()
    }
}
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
pub(crate) fn spoken(environment: &impl Environment, name: &str) -> Option<String> {
    environment.var(name).filter(|value| !value.is_empty())
}
fn spoken_parsed<T>(environment: &impl Environment, name: &str) -> Result<Option<T>, ConfigError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    match spoken(environment, name) {
        None => Ok(None),
        Some(text) => parsed(name, &text).map(Some),
    }
}
fn spoken_route(environment: &impl Environment, name: &str) -> Result<Option<Route>, ConfigError> {
    match spoken(environment, name) {
        None => Ok(None),
        Some(word) => as_route(name, &word).map(Some),
    }
}
fn spoken_wire(environment: &impl Environment, name: &str) -> Result<Option<Wire>, ConfigError> {
    match spoken(environment, name) {
        None => Ok(None),
        Some(word) => as_wire(name, &word).map(Some),
    }
}
fn spoken_window(environment: &impl Environment, name: &str) -> Result<Option<u64>, ConfigError> {
    match spoken_parsed(environment, name)? {
        None => Ok(None),
        Some(window) => nonzero_window(name, window).map(Some),
    }
}
fn parsed<T>(origin: &str, text: &str) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    text.parse::<T>().map_err(|error| ConfigError::BadValue {
        origin: origin.to_owned(),
        reason: format!("`{text}` will not parse: {error}"),
    })
}
pub const WINDOW_REFUSAL: &str = "the context window must be at least one token";
fn nonzero_window(origin: &str, window: u64) -> Result<u64, ConfigError> {
    if window == 0 {
        return Err(ConfigError::BadValue {
            origin: origin.to_owned(),
            reason: String::from(WINDOW_REFUSAL),
        });
    }
    Ok(window)
}
fn as_route(origin: &str, text: &str) -> Result<Route, ConfigError> {
    Route::from_word(text).ok_or_else(|| ConfigError::BadValue {
        origin: origin.to_owned(),
        reason: format!(
            "`{text}` is not a route; expected one of {}",
            Route::WORDS.join(", ")
        ),
    })
}
fn as_wire(origin: &str, text: &str) -> Result<Wire, ConfigError> {
    Wire::from_word(text).ok_or_else(|| ConfigError::BadValue {
        origin: origin.to_owned(),
        reason: format!(
            "`{text}` is not a wire; expected one of {}",
            Wire::WORDS.join(", ")
        ),
    })
}
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
    header_shape(name, &headers)?;
    Ok(Some(headers))
}
fn header_shape(origin: &str, headers: &[String]) -> Result<(), ConfigError> {
    match headers.iter().find(|header| !header.contains(':')) {
        Some(bad) => Err(ConfigError::BadValue {
            origin: origin.to_owned(),
            reason: format!("`{bad}` is not a header; a header is `Name: value`"),
        }),
        None => Ok(()),
    }
}
#[derive(Debug)]
pub enum ConfigError {
    UnknownKey {
        key: String,
        path: PathBuf,
    },
    BadValue {
        origin: String,
        reason: String,
    },
    Unreadable {
        path: PathBuf,
        reason: String,
    },
    Unwritable {
        path: PathBuf,
        reason: String,
    },
    NoConfigDir,
}
impl ConfigError {
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
            ConfigError::Unwritable { path, reason } => {
                write!(formatter, "cannot write {}: {reason}", path.display())
            }
            ConfigError::NoConfigDir => write!(
                formatter,
                "neither XDG_CONFIG_HOME nor HOME is set, so there is no config file to read"
            ),
        }
    }
}
impl Error for ConfigError {}
