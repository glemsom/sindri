use crate::config::{Config, ConfigError, Environment, MODE, config_path};
use std::path::{Path, PathBuf};

pub struct FirstRun {
    config: Config,
    path: PathBuf,
}

impl FirstRun {
    pub fn resolve(
        environment: &impl Environment,
        overrides: Config,
    ) -> Result<FirstRun, ConfigError> {
        let path = config_path(environment)?;
        Ok(FirstRun {
            config: Config::resolve(environment, overrides)?,
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
        let mut sources = self.config.credential_variables();
        sources.push("--api-key");
        format!(
            "api key   from {}\n          will be written to {}, mode {:04o}\n          and is never shown again",
            or_list(&sources),
            self.path.display(),
            MODE,
        )
    }
}

fn or_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [head @ .., last] => format!("{} or {last}", head.join(", ")),
    }
}
