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
//!
//! A save patches only the keys it is handed, through a temp file in the same
//! directory renamed into place, so a reader never sees a half-written file and
//! a crash mid-save leaves the file that was already there. It is owner-only,
//! because one of the keys it can hold is a secret.
//!
//! The settings modal's write is that same save with one step in front of it: the text
//! a reader typed into a row is read by the schema first (`Touched`), so a value that
//! will not do is refused naming its key before the file is read for a patch, and a row
//! a reader emptied is a key the file loses rather than one it blanks (§11, ADR-0009).

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

/// The one mode the config file is written with: owner-only, because one of the
/// keys it can hold is a secret. `MASK` comes from `redact`, so the four
/// windows share one spelling (ADR-0011).
pub const MODE: u32 = 0o600;

/// The one config schema and the one resolved config: every key optional, so
/// "unset" survives all the way down to whoever decides the default.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The wire dialect a run speaks.
    pub provider: Option<Wire>,
    /// The model a run asks for, named rather than invented.
    pub model: Option<String>,
    /// The completion endpoint a run takes, for a wire that serves two, so a
    /// route that misbehaves can be got round by name (§10).
    pub route: Option<Route>,
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
    pub const KEYS: [&'static str; 14] = [
        "provider",
        "model",
        "route",
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

    /// The keys whose value is `true` or `false`, and so the keys a screen's `Space` flips
    /// rather than types into: the one place that says which of the schema's keys is a
    /// toggle, so no screen keeps a second list that could drift (§12).
    pub const TOGGLES: [&'static str; 3] = ["thinking", "status", "ssh_agent"];

    /// Whether a run tells the terminal it is on what it is doing (§14): one switch, on
    /// unless it was turned off, read here so neither front end holds its own opinion
    /// about the default.
    pub fn tells_status(&self) -> bool {
        self.status.unwrap_or(true)
    }

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

        // Named by hand before serde sees it: `deny_unknown_fields` reports the
        // schema's shape rather than the key and the file the user has to fix.
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

    /// Writes the keys `touched` names into the one file at `path`, patching each
    /// in place so every key the save was not handed keeps its value, its
    /// comments and its place in the file.
    ///
    /// A key given as an empty string is a key a person emptied, and an emptied
    /// key is removed rather than blanked: that is what hands it back to the
    /// environment below it. A key the file never had is appended, in the order
    /// the schema names keys, so a file written from nothing reads in that order.
    ///
    /// The write is a temp file in the same directory, `fsync`, `rename`, mode
    /// `0600`. The rename is what makes it atomic: a reader of `path` sees the
    /// whole file it had or the whole file the save wrote, and a crash between
    /// the two leaves the file that was there (ADR-0009).
    pub fn save_to(path: &Path, touched: &Flags) -> Result<(), ConfigError> {
        Config::repatch(path, |document| touched.patch_into(document))
    }

    /// Writes the rows a settings screen touched into the one file at `path`: the schema
    /// reads each text first, so a value that will not parse is refused naming the key
    /// *before* anything is staged — a refusal writes nothing at all (§11) — and then the
    /// file is patched in place, exactly as a save of the flag layer is (ADR-0009).
    ///
    /// A row a reader emptied is a key the file loses rather than one it blanks, so the
    /// layer below it — the environment — speaks again (§11).
    pub fn save_touched_to(path: &Path, touched: &Touched) -> Result<(), ConfigError> {
        let (flags, emptied) = touched.patch()?;
        Config::repatch(path, |document| {
            for key in &emptied {
                document.remove(key);
            }
            flags.patch_into(document)
        })
    }

    /// Reads the file at `path`, hands its document to `patch`, and writes the whole file
    /// back through the temp file a rename makes atomic: the one body of every save.
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

        // A file this cannot parse is a file a patch would turn into nonsense, so
        // it is refused rather than rewritten: a save never clobbers what it could
        // not read.
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

    /// Writes into the one file the environment points at.
    pub fn save(environment: &impl Environment, touched: &Flags) -> Result<(), ConfigError> {
        Config::save_to(&config_path(environment)?, touched)
    }

    /// Writes what a settings screen touched into the one file the environment points at.
    pub fn save_touched(
        environment: &impl Environment,
        touched: &Touched,
    ) -> Result<(), ConfigError> {
        Config::save_touched_to(&config_path(environment)?, touched)
    }

    /// The value this layer holds for `key`, as the one text a screen shows: a wire and a
    /// route by the word they are named by, a list's items on the one line a row has, and a
    /// secret as its mask — the one value a frame may not hold (§11, ADR-0011). A key this
    /// layer leaves unset is empty, because unset is a state and not a value.
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
            "max_turns" => number_or_empty(self.max_turns),
            "timeout" => number_or_empty(self.timeout),
            "command_budget_ms" => number_or_empty(self.command_budget_ms),
            "output_budget_bytes" => number_or_empty(self.output_budget_bytes),
            "status" => on_or_empty(self.status),
            "ssh_agent" => on_or_empty(self.ssh_agent),
            _ => String::new(),
        }
    }

    /// This layer, with every key it leaves unset taken from the layer below it.
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
            route: spoken_route(environment, "SINDRI_ROUTE")?,
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

    /// The secrets this layer holds, as the set every `Redacted` scrubs through.
    ///
    /// There are exactly two doors here, and both are values a wire would echo
    /// back in its own refusal: the credential, and every `extra_headers`
    /// value. A header line is a secret whole and its value is a secret on its
    /// own, so both are kept: a provider that answers with the value alone is
    /// as loud as one that quotes the line (ADR-0011).
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
            .field("route", &self.route)
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
    /// The completion endpoint a run takes, replacing the configured one.
    pub route: Option<Route>,
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

impl Flags {
    /// Patches this touched-key set into a document, key by key in the order the
    /// schema names them. A key left unset here is a key no one touched, so the
    /// file keeps whatever it already said about it.
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
            .field("max_turns", &self.max_turns)
            .field("timeout", &self.timeout)
            .field("command_budget_ms", &self.command_budget_ms)
            .field("output_budget_bytes", &self.output_budget_bytes)
            .field("status", &self.status)
            .field("ssh_agent", &self.ssh_agent)
            .finish()
    }
}

/// The keys a settings screen touched, and the text a reader left each of them holding:
/// the patch `Enter` hands the one save (ADR-0009).
///
/// A key this does not name is a key nobody touched, so the file keeps whatever it already
/// said about it — which is what makes a visit that changed one row one line of the file.
/// An emptied text is a key a person handed back: the file loses it rather than blanking
/// it, so the environment above it speaks again (§11).
///
/// The text is what a reader typed, not a value: the schema reads it in `patch`, and a
/// text that will not parse is refused there, naming the key, before anything is staged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Touched {
    /// One row per key a reader wrote into, in the schema's own spelling.
    keys: BTreeMap<&'static str, String>,
}

/// The glyph a row separates the items of a list with: a screen's row is one line, so a
/// list of paths or of headers has to fit on it, and a comma is what a reader can see and
/// type. A path or a header value that needs a comma in it is edited in the file, which
/// stays the owner of everything a row cannot hold.
const LIST_SEPARATOR: &str = ", ";

impl Touched {
    /// A visit that has written nothing yet: a screen opened is a file untouched.
    pub fn new() -> Touched {
        Touched::default()
    }

    /// Records the text the screen holds for `key` — an empty text being a key the reader
    /// emptied, which the save is to remove.
    pub fn set(&mut self, key: &'static str, text: impl Into<String>) {
        self.keys.insert(key, text.into());
    }

    /// Whether the visit touched anything at all: a screen closed without a key pressed is
    /// a file with no save to make.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The rows this holds, in the schema's order rather than a hash's: what a test reads
    /// back, and what a screen would paint if it grew a preview of the write.
    pub fn rows(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.keys.iter().map(|(key, text)| (*key, text.as_str()))
    }

    /// The patch as the schema reads it: every touched row parsed into the flag layer, and
    /// every row a reader emptied named so the file loses it.
    ///
    /// This is the one place typed text meets the schema, and it is why a refusal can
    /// promise that nothing was written: the parse happens before the file is even read
    /// for a patch.
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

/// The items one row holds, split on the separator a row uses and read as items rather than
/// as spellings of them: the spaces around a comma are the row's, not the value's.
fn listed(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The headers one row holds: the same list a row separates with commas, each of them
/// checked for the shape every header has.
fn headers_of(key: &str, text: &str) -> Result<Vec<String>, ConfigError> {
    let headers = listed(text);
    header_shape(key, &headers)?;
    Ok(headers)
}

/// The value a row shows for a toggle: the word it holds, and nothing at all while the key
/// is unset — unset being a state and not a false.
fn on_or_empty(value: Option<bool>) -> String {
    value.map(|on| on.to_string()).unwrap_or_default()
}

/// The digits a row shows for a number, and nothing at all while the key is unset.
fn number_or_empty(number: Option<impl fmt::Display>) -> String {
    number.map(|value| value.to_string()).unwrap_or_default()
}

/// Writes one key where the file already keeps it, so the comment above the key
/// and the one beside it stay put; a key the file never had is appended.
///
/// The value is given the decor — the spacing and the trailing comment — of the
/// value it replaces, which is how one line changes and no other byte does.
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

/// Writes a string key, or removes it when the string is empty: an emptied key is
/// a key handed back to the environment, not a key blanked in the file.
fn write_string(document: &mut DocumentMut, key: &str, value: &str) {
    if value.is_empty() {
        document.remove(key);
    } else {
        write_key(document, key, Value::from(value));
    }
}

/// A list of strings as the file's own array.
fn array(values: Vec<String>) -> Value {
    let mut array = toml_edit::Array::new();
    for value in values {
        array.push(value);
    }
    Value::Array(array)
}

/// A `u64` as the file's integer, which holds only an `i64`: a number beyond that
/// is refused, naming the key, rather than written as something no read takes back.
fn integer(key: &str, value: u64) -> Result<Value, ConfigError> {
    i64::try_from(value)
        .map(Value::from)
        .map_err(|_| ConfigError::BadValue {
            origin: key.to_owned(),
            reason: format!("{value} is larger than a config file's integer"),
        })
}

/// Writes `text` at `path` through a temp file in that same directory, `fsync`ed
/// and then renamed over `path`, so the path names either the whole old file or
/// the whole new one and never a file half written.
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

    // Staged in the directory the rename lands in, so the rename is a rename
    // and never a copy across filesystems.
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("config.toml"));
    let mut staged = tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".staged")
        .tempfile_in(directory)
        .map_err(|error| unwritable(error.to_string()))?;

    // Owner-only before a byte is written, so a key is never readable by anyone
    // else even for the instant the temp file exists.
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

    // A save that cannot land leaves no litter and the file that was there untouched.
    staged
        .persist(path)
        .map_err(|error| unwritable(error.error.to_string()))?;

    // Syncing the directory is what makes the new name survive a power cut.
    // Failing to sync cannot un-write bytes already in place, so it is not
    // reported as a failed save.
    let _ = std::fs::File::open(directory).and_then(|directory| directory.sync_all());
    Ok(())
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
pub(crate) fn spoken(environment: &impl Environment, name: &str) -> Option<String> {
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
        Some(text) => parsed(name, &text).map(Some),
    }
}

/// A variable read as a route, which accepts exactly the two words.
fn spoken_route(environment: &impl Environment, name: &str) -> Result<Option<Route>, ConfigError> {
    match spoken(environment, name) {
        None => Ok(None),
        Some(word) => as_route(name, &word).map(Some),
    }
}

/// A variable read as a wire, which accepts exactly the three words.
fn spoken_wire(environment: &impl Environment, name: &str) -> Result<Option<Wire>, ConfigError> {
    match spoken(environment, name) {
        None => Ok(None),
        Some(word) => as_wire(name, &word).map(Some),
    }
}

/// A text read as some other type, naming the key or variable it came from when it will not
/// parse: the one spelling of "a value will not do", shared by the environment and by the
/// text a settings row holds.
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

/// A text read as a route, which accepts exactly the two words.
fn as_route(origin: &str, text: &str) -> Result<Route, ConfigError> {
    Route::from_word(text).ok_or_else(|| ConfigError::BadValue {
        origin: origin.to_owned(),
        reason: format!(
            "`{text}` is not a route; expected one of {}",
            Route::WORDS.join(", ")
        ),
    })
}

/// A text read as a wire, which accepts exactly the three words.
fn as_wire(origin: &str, text: &str) -> Result<Wire, ConfigError> {
    Wire::from_word(text).ok_or_else(|| ConfigError::BadValue {
        origin: origin.to_owned(),
        reason: format!(
            "`{text}` is not a wire; expected one of {}",
            Wire::WORDS.join(", ")
        ),
    })
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
    header_shape(name, &headers)?;
    Ok(Some(headers))
}

/// Whether every header is `Name: value`, refusing the first that is not, naming where it
/// was found: a header without a colon is a line a wire cannot send.
fn header_shape(origin: &str, headers: &[String]) -> Result<(), ConfigError> {
    match headers.iter().find(|header| !header.contains(':')) {
        Some(bad) => Err(ConfigError::BadValue {
            origin: origin.to_owned(),
            reason: format!("`{bad}` is not a header; a header is `Name: value`"),
        }),
        None => Ok(()),
    }
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
    /// A file that could not be written at all.
    Unwritable {
        /// The file that would not be written.
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
