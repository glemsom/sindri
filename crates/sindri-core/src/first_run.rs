use crate::config::{self, Config, ConfigError, Environment, Flags, config_path};
use crate::wire::Wire;
use std::path::Path;
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
fn or_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [head @ .., last] => format!("{} or {last}", head.join(", ")),
    }
}
pub struct FirstRun {
    config: Config,
    path: std::path::PathBuf,
}
/// The config a launch runs on: what the flags, the environment and the file resolve to, with the
/// credential the wire's own variable holds folded in. Every resolution a launch makes comes through
/// here, because `Config::resolve` alone reads no credential variable and a run would be refused.
pub fn resolved(environment: &impl Environment, flags: Flags) -> Result<Config, ConfigError> {
    let file = Config::load(environment)?;
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
    Ok(config)
}
impl FirstRun {
    pub fn resolve(environment: &impl Environment, flags: Flags) -> Result<FirstRun, ConfigError> {
        let path = config_path(environment)?;
        Ok(FirstRun {
            config: resolved(environment, flags)?,
            path,
        })
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn api_key_row(&self) -> String {
        api_key_row(self.config.provider, &self.path)
    }
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
    pub fn is_ready(&self) -> bool {
        self.missing().is_empty()
    }
}
