use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{Config, Environment};
use crate::policy::Boundary;

pub const VARIABLE: &str = "SINDRI_LAUNCH";

#[derive(Deserialize, Serialize)]
pub struct Handoff {
    pub config: Config,
    pub boundary: Boundary,
    pub workspace: PathBuf,
    pub environment: BTreeMap<String, String>,
}

impl Handoff {
    pub fn read(environment: &impl Environment) -> Result<Option<Self>, String> {
        let Some(path) = environment.var(VARIABLE) else {
            return Ok(None);
        };
        let bytes = std::fs::read(path).map_err(|_| "cannot read inherited launch".to_owned())?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "invalid inherited launch".to_owned())
    }

    pub fn write(
        config: &Config,
        boundary: &Boundary,
        workspace: &Path,
        temp: &Path,
        environment: &impl Environment,
    ) -> Result<PathBuf, String> {
        let mut inherited = BTreeMap::new();
        for name in ["HOME", "XDG_CONFIG_HOME", "PATH"] {
            if let Some(value) = environment.var(name) {
                inherited.insert(name.to_owned(), value);
            }
        }
        if config.provider == Some(crate::wire::Wire::Copilot) {
            let source = crate::provider::copilot::store_dir(environment)
                .map_err(|_| "cannot locate child signing")?
                .join("copilot.json");
            let config_home = temp.join("child-config");
            let directory = config_home.join("sindri/auth");
            std::fs::create_dir_all(&directory).map_err(|_| "cannot create child signing")?;
            if source.exists() {
                let bytes = std::fs::read(source).map_err(|_| "cannot read child signing")?;
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(directory.join("copilot.json"))
                    .and_then(|mut file| file.write_all(&bytes))
                    .map_err(|_| "cannot protect child signing")?;
            }
            inherited.insert(
                "XDG_CONFIG_HOME".to_owned(),
                config_home.display().to_string(),
            );
        }
        inherited.insert(
            "XDG_STATE_HOME".to_owned(),
            temp.join("children").display().to_string(),
        );
        let handoff = Self {
            config: config.clone(),
            boundary: boundary
                .with_temp(temp)
                .map_err(|_| "cannot preserve child writable boundary")?,
            workspace: workspace.to_owned(),
            environment: inherited,
        };
        let path = temp.join("launch.json");
        let bytes = serde_json::to_vec(&handoff).map_err(|_| "cannot encode child launch")?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut file| file.write_all(&bytes))
            .map_err(|_| "cannot protect child launch")?;
        Ok(path)
    }
}

pub fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
