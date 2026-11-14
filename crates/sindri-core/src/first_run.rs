use crate::config::{Config, ConfigError, Environment, Flags, config_path};
use std::path::{Path, PathBuf};

pub struct FirstRun {
    config: Config,
    path: PathBuf,
}

impl FirstRun {
    pub fn resolve(environment: &impl Environment, flags: Flags) -> Result<FirstRun, ConfigError> {
        let path = config_path(environment)?;
        Ok(FirstRun {
            config: Config::resolve(environment, flags)?,
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
        self.config.api_key_row(&self.path)
    }
}
