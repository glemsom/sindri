use crate::provider::Credential;
use crate::provider::Route;
use crate::provider::endpoint::Endpoint;
use crate::redact::{MASK, SecretSet};
use crate::wire::Wire;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fmt;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use toml_edit::{DocumentMut, Item, Value};
pub const MODE: u32 = 0o600;
#[derive(Clone, Default, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub provider: Option<Wire>,
    pub model: Option<String>,
    pub route: Option<Route>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub thinking: Option<bool>,
    pub responses_continuation: Option<bool>,
    pub responses_store: Option<bool>,
    pub responses_continuation_verified: Option<bool>,
    pub native_compaction: Option<bool>,
    pub native_compaction_verified: Option<bool>,
    pub extra_writable_paths: Option<Vec<PathBuf>>,
    pub extra_headers: Option<Vec<String>>,
    pub context_window: Option<u64>,
    pub output_headroom: Option<u64>,
    pub tool_result_headroom: Option<u64>,
    pub max_turns: Option<u32>,
    pub timeout: Option<u64>,
    pub command_budget_ms: Option<u64>,
    pub output_budget_bytes: Option<u64>,
    pub status: Option<bool>,
    pub face: Option<bool>,
    pub ssh_agent: Option<bool>,
}

type Shown = fn(&Config) -> String;
type Reading = fn(&'static str, &mut Config, &str) -> Result<(), ConfigError>;
type Writing = fn(&'static str, &Config, &mut DocumentMut) -> Result<(), ConfigError>;
type Merging = fn(&mut Config, &Config);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Boolean,
    Text,
    Integer,
    DurationSeconds,
    DurationMilliseconds,
    ByteSize,
    Url,
    Credential,
    Paths,
    Headers,
    Choice,
}

#[derive(Debug)]
pub struct Field {
    pub name: &'static str,
    pub variable: &'static str,
    pub kind: FieldKind,
    shown: Shown,
    reading: Reading,
    writing: Writing,
    spoken: Reading,
    merging: Merging,
}

macro_rules! or_lower {
    ($key:ident) => {
        |config: &mut Config, lower: &Config| {
            config.$key = config.$key.take().or_else(|| lower.$key.clone());
        }
    };
}

impl Field {
    const fn typed(
        name: &'static str,
        variable: &'static str,
        kind: FieldKind,
        shown: Shown,
        reading: Reading,
        writing: Writing,
        merging: Merging,
    ) -> Field {
        Field {
            name,
            variable,
            kind,
            shown,
            reading,
            writing,
            spoken: reading,
            merging,
        }
    }

    const fn spoken(mut self, spoken: Reading) -> Field {
        self.spoken = spoken;
        self
    }

    pub fn choices(&self, wire: Option<Wire>) -> Option<Vec<&'static str>> {
        if self.kind == FieldKind::Boolean {
            return Some(vec!["true", "false", ""]);
        }
        match self.name {
            "provider" => Some(Wire::WORDS.to_vec()),
            "route" => Some(if wire == Some(Wire::Anthropic) {
                Vec::new()
            } else {
                std::iter::once("").chain(Route::WORDS).collect()
            }),
            _ => None,
        }
    }

    pub fn switch(&self) -> bool {
        self.kind == FieldKind::Boolean
    }

    pub fn shown(&self, config: &Config) -> String {
        (self.shown)(config)
    }

    pub fn speak(&self, text: &str, config: &mut Config) -> Result<(), ConfigError> {
        (self.spoken)(self.variable, config, text)
    }

    pub fn or_lower(&self, config: &mut Config, lower: &Config) {
        (self.merging)(config, lower)
    }

    pub fn read(&self, text: &str) -> Result<Config, ConfigError> {
        let mut config = Config::default();
        if text.is_empty() && self.kind == FieldKind::Boolean {
            return Ok(config);
        }
        let normalized = match self.kind {
            FieldKind::DurationSeconds => {
                Some(quantity(self.name, text, false, 1_000_000_000)?.to_string())
            }
            FieldKind::DurationMilliseconds => {
                Some(quantity(self.name, text, false, 1_000_000)?.to_string())
            }
            FieldKind::ByteSize => Some(quantity(self.name, text, true, 1)?.to_string()),
            FieldKind::Url => {
                let url = text.parse::<http::Uri>().map_err(|_| {
                    bad_input(
                        self.name,
                        "expected an absolute http or https URL with a host",
                    )
                })?;
                if !matches!(url.scheme_str(), Some("http" | "https"))
                    || url.host().is_none_or(str::is_empty)
                {
                    return Err(bad_input(
                        self.name,
                        "expected an absolute http or https URL with a host",
                    ));
                }
                None
            }
            _ => None,
        };
        (self.reading)(
            self.name,
            &mut config,
            normalized.as_deref().unwrap_or(text),
        )?;
        if self.kind == FieldKind::Integer {
            integer(self.name, parsed(self.name, text)?)?;
        }
        Ok(config)
    }

    pub fn write(&self, config: &Config, document: &mut DocumentMut) -> Result<(), ConfigError> {
        (self.writing)(self.name, config, document)
    }
}

impl Config {
    pub const SCHEMA: [Field; 23] = [
        Field::typed(
            "provider",
            "SINDRI_PROVIDER",
            FieldKind::Choice,
            |config| word_or_empty(config.provider.map(Wire::word)),
            |key, config, text| {
                config.provider = Some(as_wire(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.provider.map(Wire::word), document),
            or_lower!(provider),
        ),
        Field::typed(
            "model",
            "SINDRI_MODEL",
            FieldKind::Text,
            |config| string_or_empty(&config.model),
            |_, config, text| {
                config.model = Some(text.to_owned());
                Ok(())
            },
            |key, config, document| write_text(key, config.model.as_ref(), document),
            or_lower!(model),
        ),
        Field::typed(
            "route",
            "SINDRI_ROUTE",
            FieldKind::Choice,
            |config| word_or_empty(config.route.map(Route::word)),
            |key, config, text| {
                config.route = Some(as_route(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.route.map(Route::word), document),
            or_lower!(route),
        ),
        Field::typed(
            "base_url",
            "SINDRI_BASE_URL",
            FieldKind::Url,
            |config| string_or_empty(&config.base_url),
            |_, config, text| {
                config.base_url = Some(text.to_owned());
                Ok(())
            },
            |key, config, document| write_text(key, config.base_url.as_ref(), document),
            or_lower!(base_url),
        ),
        Field::typed(
            "api_key",
            "SINDRI_API_KEY",
            FieldKind::Credential,
            |config| {
                if config.api_key.is_some() {
                    MASK.to_owned()
                } else {
                    String::new()
                }
            },
            |key, config, text| {
                config.api_key = Some(secret(key, text)?);
                Ok(())
            },
            |key, config, document| write_text(key, config.api_key.as_ref(), document),
            or_lower!(api_key),
        ),
        Field::typed(
            "responses_continuation",
            "SINDRI_RESPONSES_CONTINUATION",
            FieldKind::Boolean,
            |config| on_or_empty(config.responses_continuation),
            |key, config, text| {
                config.responses_continuation = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.responses_continuation, document),
            or_lower!(responses_continuation),
        ),
        Field::typed(
            "responses_store",
            "SINDRI_RESPONSES_STORE",
            FieldKind::Boolean,
            |config| on_or_empty(config.responses_store),
            |key, config, text| {
                config.responses_store = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.responses_store, document),
            or_lower!(responses_store),
        ),
        Field::typed(
            "responses_continuation_verified",
            "SINDRI_RESPONSES_CONTINUATION_VERIFIED",
            FieldKind::Boolean,
            |config| on_or_empty(config.responses_continuation_verified),
            |key, config, text| {
                config.responses_continuation_verified = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| {
                write_value(key, config.responses_continuation_verified, document)
            },
            or_lower!(responses_continuation_verified),
        ),
        Field::typed(
            "native_compaction",
            "SINDRI_NATIVE_COMPACTION",
            FieldKind::Boolean,
            |config| on_or_empty(config.native_compaction),
            |key, config, text| {
                config.native_compaction = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.native_compaction, document),
            or_lower!(native_compaction),
        ),
        Field::typed(
            "native_compaction_verified",
            "SINDRI_NATIVE_COMPACTION_VERIFIED",
            FieldKind::Boolean,
            |config| on_or_empty(config.native_compaction_verified),
            |key, config, text| {
                config.native_compaction_verified = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.native_compaction_verified, document),
            or_lower!(native_compaction_verified),
        ),
        Field::typed(
            "thinking",
            "SINDRI_THINKING",
            FieldKind::Boolean,
            |config| on_or_empty(config.thinking),
            |key, config, text| {
                config.thinking = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.thinking, document),
            or_lower!(thinking),
        ),
        Field::typed(
            "extra_writable_paths",
            "SINDRI_EXTRA_WRITABLE_PATHS",
            FieldKind::Paths,
            |config| {
                let paths = config.extra_writable_paths.as_deref().unwrap_or_default();
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<String>>()
                    .join(LIST_SEPARATOR)
            },
            |_, config, text| {
                config.extra_writable_paths =
                    Some(listed(text).into_iter().map(PathBuf::from).collect());
                Ok(())
            },
            |key, config, document| {
                write_paths(key, config.extra_writable_paths.as_deref(), document)
            },
            or_lower!(extra_writable_paths),
        )
        .spoken(|_, config, text| {
            config.extra_writable_paths = Some(env::split_paths(text).collect());
            Ok(())
        }),
        Field::typed(
            "extra_headers",
            "SINDRI_EXTRA_HEADERS",
            FieldKind::Headers,
            |config| {
                config
                    .extra_headers
                    .as_deref()
                    .unwrap_or_default()
                    .join(LIST_SEPARATOR)
            },
            |key, config, text| {
                config.extra_headers = Some(headers_of(key, text)?);
                Ok(())
            },
            |key, config, document| write_headers(key, config.extra_headers.as_deref(), document),
            or_lower!(extra_headers),
        )
        .spoken(|variable, config, text| {
            let headers: Vec<String> = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect();
            header_shape(variable, &headers)?;
            config.extra_headers = Some(headers);
            Ok(())
        }),
        Field::typed(
            "context_window",
            "SINDRI_CONTEXT_WINDOW",
            FieldKind::Integer,
            |config| number_or_empty(config.context_window),
            |key, config, text| {
                config.context_window = Some(nonzero_window(key, parsed(key, text)?)?);
                Ok(())
            },
            |key, config, document| write_window(key, config.context_window, document),
            or_lower!(context_window),
        ),
        Field::typed(
            "output_headroom",
            "SINDRI_OUTPUT_HEADROOM",
            FieldKind::Integer,
            |config| number_or_empty(config.output_headroom),
            |key, config, text| {
                config.output_headroom = Some(nonzero_window(key, parsed(key, text)?)?);
                Ok(())
            },
            |key, config, document| write_window(key, config.output_headroom, document),
            or_lower!(output_headroom),
        ),
        Field::typed(
            "tool_result_headroom",
            "SINDRI_TOOL_RESULT_HEADROOM",
            FieldKind::Integer,
            |config| number_or_empty(config.tool_result_headroom),
            |key, config, text| {
                config.tool_result_headroom = Some(nonzero_window(key, parsed(key, text)?)?);
                Ok(())
            },
            |key, config, document| write_window(key, config.tool_result_headroom, document),
            or_lower!(tool_result_headroom),
        ),
        Field::typed(
            "max_turns",
            "SINDRI_MAX_TURNS",
            FieldKind::Integer,
            |config| number_or_empty(config.max_turns),
            |key, config, text| {
                config.max_turns = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_number(key, config.max_turns.map(u64::from), document),
            or_lower!(max_turns),
        ),
        Field::typed(
            "timeout",
            "SINDRI_TIMEOUT",
            FieldKind::DurationSeconds,
            |config| number_or_empty(config.timeout),
            |key, config, text| {
                config.timeout = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_number(key, config.timeout, document),
            or_lower!(timeout),
        ),
        Field::typed(
            "command_budget_ms",
            "SINDRI_COMMAND_BUDGET_MS",
            FieldKind::DurationMilliseconds,
            |config| number_or_empty(config.command_budget_ms),
            |key, config, text| {
                config.command_budget_ms = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_number(key, config.command_budget_ms, document),
            or_lower!(command_budget_ms),
        ),
        Field::typed(
            "output_budget_bytes",
            "SINDRI_OUTPUT_BUDGET_BYTES",
            FieldKind::ByteSize,
            |config| number_or_empty(config.output_budget_bytes),
            |key, config, text| {
                config.output_budget_bytes = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_number(key, config.output_budget_bytes, document),
            or_lower!(output_budget_bytes),
        ),
        Field::typed(
            "status",
            "SINDRI_STATUS",
            FieldKind::Boolean,
            |config| on_or_empty(config.status),
            |key, config, text| {
                config.status = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.status, document),
            or_lower!(status),
        ),
        Field::typed(
            "face",
            "SINDRI_FACE",
            FieldKind::Boolean,
            |config| on_or_empty(config.face),
            |key, config, text| {
                config.face = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.face, document),
            or_lower!(face),
        ),
        Field::typed(
            "ssh_agent",
            "SINDRI_SSH_AGENT",
            FieldKind::Boolean,
            |config| on_or_empty(config.ssh_agent),
            |key, config, text| {
                config.ssh_agent = Some(parsed(key, text)?);
                Ok(())
            },
            |key, config, document| write_value(key, config.ssh_agent, document),
            or_lower!(ssh_agent),
        ),
    ];

    pub fn field(name: &str) -> Option<&'static Field> {
        Config::SCHEMA.iter().find(|field| field.name == name)
    }

    pub fn tells_status(&self) -> bool {
        self.status.unwrap_or(true)
    }
    pub fn decides_the_same_run(&self, other: &Config) -> bool {
        fn run_only(config: &Config) -> Config {
            Config {
                status: None,
                face: None,
                ..config.clone()
            }
        }
        run_only(self) == run_only(other)
    }
    pub fn draws_face(&self) -> bool {
        self.face.unwrap_or(true)
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
        if let Some(key) = table.keys().find(|key| Config::field(key).is_none()) {
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
        for (key, value) in [
            ("output_headroom", config.output_headroom),
            ("tool_result_headroom", config.tool_result_headroom),
        ] {
            if let Some(value) = value {
                nonzero_window(key, value)?;
            }
        }
        Ok(config)
    }
    pub fn load(environment: &impl Environment) -> Result<Config, ConfigError> {
        Config::load_from(&config_path(environment)?)
    }
    pub fn save_to(path: &Path, overrides: &Config) -> Result<(), ConfigError> {
        Config::repatch(path, |document| overrides.patch_into(document))
    }
    pub fn save_touched_to(path: &Path, touched: &Touched) -> Result<(), ConfigError> {
        let (written, emptied) = touched.patch()?;
        Config::repatch(path, |document| {
            for key in &emptied {
                document.remove(key);
            }
            written.patch_into(document)
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
    pub fn save(environment: &impl Environment, overrides: &Config) -> Result<(), ConfigError> {
        Config::save_to(&config_path(environment)?, overrides)
    }
    pub fn save_touched(
        environment: &impl Environment,
        touched: &Touched,
    ) -> Result<(), ConfigError> {
        Config::save_touched_to(&config_path(environment)?, touched)
    }
    fn patch_into(&self, document: &mut DocumentMut) -> Result<(), ConfigError> {
        for field in &Config::SCHEMA {
            field.write(self, document)?;
        }
        Ok(())
    }
    pub fn above(mut self, lower: Config) -> Config {
        for field in &Config::SCHEMA {
            field.or_lower(&mut self, &lower);
        }
        self
    }
    pub fn from_environment(environment: &impl Environment) -> Result<Config, ConfigError> {
        let mut config = Config::default();
        for field in &Config::SCHEMA {
            let Some(text) = spoken(environment, field.variable) else {
                continue;
            };
            field.speak(&text, &mut config)?;
        }
        Ok(config)
    }

    pub fn endpoint(&self) -> Option<Endpoint> {
        let wire = self.provider?;
        let endpoint = match &self.base_url {
            Some(base_url) => Endpoint::at(wire, base_url.clone()),
            None => Endpoint::own(wire),
        };
        Some(endpoint.routed(self.route))
    }

    pub fn credential(&self) -> Credential {
        match &self.api_key {
            Some(key) => Credential::new(key.clone()),
            None => Credential::none(),
        }
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
    pub fn resolve(
        environment: &impl Environment,
        overrides: Config,
    ) -> Result<Config, ConfigError> {
        Ok(crate::launch::Preparation::resolve(environment, overrides)?.into_config())
    }

    pub(crate) fn resolve_ambient(
        environment: &impl Environment,
        overrides: Config,
    ) -> Result<Config, ConfigError> {
        let file = Config::load(environment)?;
        let above = overrides.above(Config::from_environment(environment)?);
        let wire_key = above
            .provider
            .or(file.provider)
            .and_then(Wire::credential_variable)
            .and_then(|name| spoken(environment, name));
        let spoken_key = above.api_key.clone();
        let written_key = file.api_key.clone();
        let mut config = above.above(file);
        config.api_key = spoken_key.or(wire_key).or(written_key);
        Ok(config)
    }
    pub fn credential_variables(&self) -> Vec<&'static str> {
        let mut variables: Vec<&'static str> = self
            .provider
            .and_then(Wire::credential_variable)
            .into_iter()
            .collect();
        variables.extend(Config::field("api_key").map(|field| field.variable));
        variables
    }
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.provider.is_none() {
            missing.push("provider");
        }
        if self.model.is_none() {
            missing.push("model");
        }
        let key_asked_for = self
            .provider
            .is_some_and(|wire| wire.credential_variable().is_some());
        if key_asked_for && self.api_key.is_none() {
            missing.push("api_key");
        }
        missing
    }
    pub fn is_ready(&self) -> bool {
        self.missing().is_empty()
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
            .field("face", &self.face)
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
    fn patch(&self) -> Result<(Config, Vec<&'static str>), ConfigError> {
        let mut written = Config::default();
        let mut emptied: Vec<&'static str> = Vec::new();
        for (key, held) in &self.keys {
            let field = Config::field(key).ok_or_else(|| ConfigError::BadValue {
                origin: (*key).to_owned(),
                reason: String::from("not a key of the config file"),
            })?;
            let text = held.trim();
            if text.is_empty() {
                emptied.push(field.name);
                continue;
            }
            written = field.read(text)?.above(written);
        }
        Ok((written, emptied))
    }
}

fn listed(text: &str) -> Vec<String> {
    if let Ok(Value::Array(values)) = text.parse::<Value>()
        && values.iter().all(|value| value.as_str().is_some())
    {
        return values
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
    }
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
fn secret(key: &str, text: &str) -> Result<String, ConfigError> {
    if text == MASK {
        return Err(ConfigError::BadValue {
            origin: key.to_owned(),
            reason: String::from("the mask a frame shows for a secret is not a secret"),
        });
    }
    Ok(text.to_owned())
}
fn string_or_empty(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}
fn word_or_empty(word: Option<&'static str>) -> String {
    word.map(str::to_owned).unwrap_or_default()
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
fn write_value(
    key: &str,
    value: Option<impl Into<Value>>,
    document: &mut DocumentMut,
) -> Result<(), ConfigError> {
    if let Some(value) = value {
        write_key(document, key, value.into());
    }
    Ok(())
}
fn write_text(
    key: &str,
    value: Option<&String>,
    document: &mut DocumentMut,
) -> Result<(), ConfigError> {
    if let Some(value) = value {
        write_string(document, key, value);
    }
    Ok(())
}
fn write_number(
    key: &str,
    value: Option<u64>,
    document: &mut DocumentMut,
) -> Result<(), ConfigError> {
    if let Some(value) = value {
        write_key(document, key, integer(key, value)?);
    }
    Ok(())
}
fn write_window(
    key: &str,
    value: Option<u64>,
    document: &mut DocumentMut,
) -> Result<(), ConfigError> {
    if let Some(value) = value {
        write_key(document, key, integer(key, nonzero_window(key, value)?)?);
    }
    Ok(())
}
fn write_paths(
    key: &str,
    paths: Option<&[PathBuf]>,
    document: &mut DocumentMut,
) -> Result<(), ConfigError> {
    if let Some(paths) = paths {
        let strings: Vec<String> = paths
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        write_key(document, key, array(strings));
    }
    Ok(())
}
fn write_headers(
    key: &str,
    headers: Option<&[String]>,
    document: &mut DocumentMut,
) -> Result<(), ConfigError> {
    if let Some(headers) = headers {
        write_key(document, key, array(headers.to_vec()));
    }
    Ok(())
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

impl<T: Environment + ?Sized> Environment for &T {
    fn var(&self, name: &str) -> Option<String> {
        (**self).var(name)
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
fn bad_input(origin: &str, reason: &str) -> ConfigError {
    ConfigError::BadValue {
        origin: origin.to_owned(),
        reason: reason.to_owned(),
    }
}

fn quantity(origin: &str, text: &str, bytes: bool, divisor: u128) -> Result<u64, ConfigError> {
    let invalid = || {
        bad_input(
            origin,
            "expected a nonnegative quantity in whole config units within the TOML integer range",
        )
    };
    let text = text.trim();
    if let Ok(value) = text.parse::<u64>() {
        integer(origin, value)?;
        return Ok(value);
    }
    let mut rest = text;
    let mut total = 0u128;
    let mut count = 0;
    while !rest.is_empty() {
        let end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        let number = &rest[..end];
        let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
        if whole.is_empty()
            || !whole.bytes().all(|c| c.is_ascii_digit())
            || !fraction.bytes().all(|c| c.is_ascii_digit())
            || fraction.len() > 18
        {
            return Err(invalid());
        }
        let scale = 10u128
            .checked_pow(fraction.len() as u32)
            .ok_or_else(invalid)?;
        let numerator = whole
            .parse::<u128>()
            .map_err(|_| invalid())?
            .checked_mul(scale)
            .and_then(|n| n.checked_add(fraction.parse::<u128>().unwrap_or(0)))
            .ok_or_else(invalid)?;
        rest = rest[end..].trim_start();
        let end = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let unit = rest[..end].to_ascii_lowercase();
        let multiplier: u128 = if bytes {
            match unit.as_str() {
                "b" | "byte" | "bytes" => 1,
                "kb" => 1_000,
                "mb" => 1_000_000,
                "gb" => 1_000_000_000,
                "tb" => 1_000_000_000_000,
                "kib" => 1 << 10,
                "mib" => 1 << 20,
                "gib" => 1 << 30,
                "tib" => 1 << 40,
                _ => return Err(invalid()),
            }
        } else {
            match unit.as_str() {
                "ns" => 1,
                "us" => 1_000,
                "ms" | "millisecond" | "milliseconds" => 1_000_000,
                "s" | "sec" | "secs" | "second" | "seconds" => 1_000_000_000,
                "m" | "min" | "mins" | "minute" | "minutes" => 60_000_000_000,
                "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000_000_000,
                "d" | "day" | "days" => 86_400_000_000_000,
                _ => return Err(invalid()),
            }
        };
        let scaled = numerator.checked_mul(multiplier).ok_or_else(invalid)?;
        if scaled % scale != 0 {
            return Err(invalid());
        }
        total = total.checked_add(scaled / scale).ok_or_else(invalid)?;
        count += 1;
        rest = rest[end..].trim_start();
        if bytes && !rest.is_empty() {
            return Err(invalid());
        }
    }
    if count == 0 || !total.is_multiple_of(divisor) {
        return Err(invalid());
    }
    let value = u64::try_from(total / divisor).map_err(|_| invalid())?;
    integer(origin, value)?;
    Ok(value)
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
fn header_shape(origin: &str, headers: &[String]) -> Result<(), ConfigError> {
    match headers.iter().find(|header| {
        let Some((name, value)) = header.split_once(':') else {
            return true;
        };
        name.parse::<http::header::HeaderName>().is_err()
            || http::header::HeaderValue::from_str(value.trim()).is_err()
    }) {
        Some(bad) => Err(ConfigError::BadValue {
            origin: origin.to_owned(),
            reason: format!("`{bad}` is not a header; a header is `Name: value`"),
        }),
        None => Ok(()),
    }
}
#[derive(Debug)]
pub enum ConfigError {
    UnknownKey { key: String, path: PathBuf },
    BadValue { origin: String, reason: String },
    Unreadable { path: PathBuf, reason: String },
    Unwritable { path: PathBuf, reason: String },
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

pub fn collection_text(items: &[String]) -> String {
    if items.iter().any(|item| item.contains(',')) {
        let mut array = toml_edit::Array::new();
        for item in items {
            array.push(item.as_str());
        }
        array.to_string()
    } else {
        items.join(", ")
    }
}
