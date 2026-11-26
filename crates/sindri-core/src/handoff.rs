use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
        let mut file =
            std::fs::File::open(&path).map_err(|_| "cannot read inherited launch".to_owned())?;
        let seals = unsafe { fcntl(file.as_raw_fd(), GET_SEALS, 0) };
        let readonly_mount = std::fs::read_to_string("/proc/self/mountinfo").is_ok_and(|mounts| {
            mounts.lines().any(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                fields
                    .get(4)
                    .is_some_and(|target| *target == path && target.ends_with("/launch.json"))
                    && fields
                        .get(5)
                        .is_some_and(|flags| flags.split(',').any(|flag| flag == "ro"))
            })
        });
        if !readonly_mount && (seals < 0 || seals & REQUIRED_SEALS != REQUIRED_SEALS) {
            return Err("untrusted inherited launch".to_owned());
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|_| "cannot read inherited launch".to_owned())?;
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
    ) -> Result<Transport, String> {
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
        let bytes = serde_json::to_vec(&handoff).map_err(|_| "cannot encode child launch")?;
        let fd = unsafe { memfd_create(c"sindri-launch".as_ptr(), 3) };
        if fd < 0 {
            return Err("cannot create immutable child launch".to_owned());
        }
        let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
        file.write_all(&bytes)
            .map_err(|_| "cannot write child launch")?;
        if unsafe { fchmod(fd, 0o600) } != 0 || unsafe { fcntl(fd, ADD_SEALS, REQUIRED_SEALS) } != 0
        {
            return Err("cannot seal child launch".to_owned());
        }
        std::fs::write(temp.join("launch.json"), [])
            .map_err(|_| "cannot prepare child launch mount")?;
        Ok(Transport {
            file: Arc::new(file),
            environment: handoff.environment,
            path: temp.join("launch.json"),
            sandboxed: boundary.enforcement() == crate::policy::Enforcement::Bubblewrap,
        })
    }
}

pub fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

const ADD_SEALS: i32 = 1033;
const GET_SEALS: i32 = 1034;
const REQUIRED_SEALS: i32 = 1 | 2 | 4 | 8;

unsafe extern "C" {
    fn memfd_create(name: *const std::ffi::c_char, flags: u32) -> i32;
    fn fcntl(fd: i32, command: i32, value: i32) -> i32;
    fn fchmod(fd: i32, mode: u32) -> i32;
}

#[derive(Clone, Debug)]
pub struct Transport {
    file: Arc<std::fs::File>,
    environment: BTreeMap<String, String>,
    path: PathBuf,
    sandboxed: bool,
}

#[derive(Clone, Debug)]
pub struct ChildLaunch {
    pub executable: PathBuf,
    pub transport: Transport,
}

impl ChildLaunch {
    pub fn path(&self) -> &Path {
        &self.transport.path
    }

    pub fn descriptor(&self) -> i32 {
        self.transport.file.as_raw_fd()
    }

    pub fn command(&self, temp: &Path, command: &str) -> String {
        let exports = self
            .transport
            .environment
            .iter()
            .map(|(name, value)| format!("{name}={}", quoted(value)))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "export SINDRI_EXECUTABLE={} SINDRI_LAUNCH={} TMPDIR={} {}; {}",
            quoted(&self.executable.display().to_string()),
            quoted(&if self.transport.sandboxed {
                self.path().display().to_string()
            } else {
                format!("/proc/{}/fd/{}", std::process::id(), self.descriptor())
            }),
            quoted(&temp.display().to_string()),
            exports,
            command
        )
    }
}
