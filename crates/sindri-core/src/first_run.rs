//! The first run: no file at all, a key from the environment or a flag, and the
//! API-key row that says where a key comes from, where it will be written, the
//! mode of the file that holds it, and that the value is never shown again.
//!
//! The row is a value rather than a line printed from inside a front end, so the
//! terminal UI can paint the same bytes the batch front end writes and a test can
//! assert on them: one message, one place, two front ends.
//!
//! Resolution here is the config's own layering, file then `SINDRI_*` environment
//! then flags, with one thing added: a key the layer chain never named is read off
//! the wire's own variable (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`), so a person who
//! has exported one has supplied a key without being told to move it (§10).

use std::path::Path;

use crate::config::{self, Config, ConfigError, Environment, Flags, config_path};
use crate::wire::Wire;

/// The row that says where a key comes from, where it will be written, the mode
/// of that file, and that the value is never shown again.
///
/// A wire with no variable of its own — `copilot`, which signs in rather than
/// holding a key — leaves `SINDRI_API_KEY` and the flag as the only two sources
/// worth naming, and the row says so rather than inventing a variable.
pub fn api_key_row(wire: Option<Wire>, path: &Path) -> String {
    let mut sources: Vec<&str> = Vec::new();
    if let Some(variable) = wire.and_then(Wire::credential_variable) {
        sources.push(variable);
    }
    sources.push("SINDRI_API_KEY");
    sources.push("--api-key");

    format!(
        "api key   from {}\n          will be written to {}, mode {:04o}\n          and is never shown again",
        or_list(&sources),
        path.display(),
        config::MODE,
    )
}

/// The sources as a sentence names them: `A or B`, or `A, B or C`.
fn or_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [head @ .., last] => format!("{} or {last}", head.join(", ")),
    }
}

/// A first run's own resolution: the config as the layers left it, plus the file
/// the key would be written to, so the row and the save name the same path.
pub struct FirstRun {
    /// The config as the file, the environment and the flags left it.
    config: Config,
    /// The one file the key would be written to.
    path: std::path::PathBuf,
}

impl FirstRun {
    /// Resolves the config with no assumption of a file, reading a key off the
    /// wire's own variable when the layers above the file never named one.
    ///
    /// That variable is an environment variable, so it sits at the environment's
    /// layer: above a key in the file, below `SINDRI_API_KEY` and `--api-key`,
    /// which name Sindri rather than the wire. A resolved config cannot say which
    /// layer a key came from, so the key's precedence is spelled out here instead
    /// of being recovered afterwards.
    pub fn resolve(environment: &impl Environment, flags: Flags) -> Result<FirstRun, ConfigError> {
        let path = config_path(environment)?;
        let file = Config::load_from(&path)?;
        // The layers above the file, kept apart so the wire can be read off them
        // before the file's key is consulted.
        let above = Config::from(flags).above(Config::from_environment(environment)?);
        let wire_key = above
            .provider
            .or(file.provider)
            .and_then(Wire::credential_variable)
            .and_then(|name| config::spoken(environment, name));
        let key = above
            .api_key
            .clone()
            .or(wire_key)
            .or_else(|| file.api_key.clone());

        let mut config = above.above(file);
        config.api_key = key;
        Ok(FirstRun { config, path })
    }

    /// The config as the layers left it.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The one file a key would be written to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// This run's API-key row.
    pub fn api_key_row(&self) -> String {
        api_key_row(self.config.provider, &self.path)
    }

    /// The config keys a run still needs, in the order the row would ask for them:
    /// nothing here is a refusal, it is what the next question is about.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.config.provider.is_none() {
            missing.push("provider");
        }
        if self.config.model.is_none() {
            missing.push("model");
        }
        let key_asked_for = self
            .config
            .provider
            .is_some_and(|wire| wire.credential_variable().is_some());
        if key_asked_for && self.config.api_key.is_none() {
            missing.push("api_key");
        }
        missing
    }

    /// Whether a run has everything it needs: a wire, a model and, for a wire that
    /// holds one, a key.
    pub fn is_ready(&self) -> bool {
        self.missing().is_empty()
    }
}
