use crate::config::{Config, ConfigError, Environment, MODE, config_path};
use std::path::{Path, PathBuf};

pub struct FirstRun {
    preparation: crate::launch::Preparation,
    path: PathBuf,
}

impl FirstRun {
    pub fn resolve(
        environment: &impl Environment,
        overrides: Config,
    ) -> Result<FirstRun, ConfigError> {
        let preparation = crate::launch::Preparation::resolve(environment, overrides)?;
        let path = config_path(&preparation.environment(environment))?;
        Ok(FirstRun { preparation, path })
    }

    pub fn config(&self) -> &Config {
        self.preparation.config()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn api_key_row(&self) -> String {
        let mut sources = self.config().credential_variables();
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
