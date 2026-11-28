use std::error::Error;
use std::fmt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::Serialize;

use crate::config::{Config, Environment};

pub const DEFAULT_TIMEOUT_SECONDS: u64 = 120;

pub const MAX_TIMEOUT_SECONDS: u64 = 3_600;

pub const DEFAULT_COMMAND_BUDGET_MS: u64 = 3_600_000;

pub const DEFAULT_OUTPUT_BUDGET_BYTES: u64 = 8 * 1024 * 1024;

pub const SYSTEM_SSH_CONFIG_D: &str = "/etc/ssh/ssh_config.d";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Enforcement {
    Bubblewrap,
    Unsafe,
}

impl Enforcement {
    pub fn word(self) -> &'static str {
        match self {
            Enforcement::Bubblewrap => "bubblewrap",
            Enforcement::Unsafe => "unsafe",
        }
    }

    pub fn rail_label(self) -> &'static str {
        match self {
            Enforcement::Bubblewrap => "bwrap",
            Enforcement::Unsafe => "unsafe",
        }
    }
}

impl fmt::Display for Enforcement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.word())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct WritableSet {
    paths: Vec<PathBuf>,
}

impl WritableSet {
    /// Canonical is read against the filesystem and not as string tidying: a path is resolved
    /// through its symlinks and its `..`, so two spellings of one directory are one directory.
    pub fn new<I, P>(paths: I) -> Result<WritableSet, BoundaryError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut resolved: Vec<PathBuf> = Vec::new();
        for path in paths {
            let path = path.as_ref();
            if !path.is_absolute() {
                return Err(BoundaryError::NotAbsolute {
                    path: path.to_path_buf(),
                });
            }
            let canonical =
                std::fs::canonicalize(path).map_err(|error| BoundaryError::Unresolvable {
                    path: path.to_path_buf(),
                    reason: error.to_string(),
                })?;
            if canonical == Path::new("/") {
                return Err(BoundaryError::Root { path: canonical });
            }
            resolved.push(canonical);
        }

        // Both checks below read adjacent pairs only because of this sort: a duplicate lands
        // beside its twin, and a path's ancestor always sorts before it.
        resolved.sort();

        for pair in resolved.windows(2) {
            if pair[0] == pair[1] {
                return Err(BoundaryError::Duplicate {
                    path: pair[1].clone(),
                });
            }
        }

        for pair in resolved.windows(2) {
            if pair[1].starts_with(&pair[0]) {
                return Err(BoundaryError::Nested {
                    path: pair[1].clone(),
                    inside: pair[0].clone(),
                });
            }
        }

        Ok(WritableSet { paths: resolved })
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Boundary {
    writable: WritableSet,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    known_hosts: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ssh_config: Option<PathBuf>,
    timeout_seconds: u64,
    command_budget_ms: u64,
    output_budget_bytes: u64,
    enforcement: Enforcement,
}

impl Boundary {
    pub fn from_config(
        config: &Config,
        workspace: &Path,
        enforcement: Enforcement,
    ) -> Result<Boundary, BoundaryError> {
        let mut paths: Vec<PathBuf> = vec![workspace.to_path_buf()];
        paths.extend(config.extra_writable_paths.iter().flatten().cloned());

        let timeout_seconds = config.timeout.unwrap_or(DEFAULT_TIMEOUT_SECONDS);
        if timeout_seconds > MAX_TIMEOUT_SECONDS {
            return Err(BoundaryError::TimeoutAboveCeiling {
                seconds: timeout_seconds,
                ceiling: MAX_TIMEOUT_SECONDS,
            });
        }

        Ok(Boundary {
            writable: WritableSet::new(paths)?,
            known_hosts: None,
            ssh_config: None,
            timeout_seconds,
            command_budget_ms: config
                .command_budget_ms
                .unwrap_or(DEFAULT_COMMAND_BUDGET_MS),
            output_budget_bytes: config
                .output_budget_bytes
                .unwrap_or(DEFAULT_OUTPUT_BUDGET_BYTES),
            enforcement,
        })
    }

    /// The one file outside the declared set a forwarded agent writes: `known_hosts`, and
    /// never the directory it sits in.
    pub fn with_known_hosts(mut self, known_hosts: Option<PathBuf>) -> Self {
        self.known_hosts = known_hosts;
        self
    }

    /// The staged drop-ins the sandbox mounts over `SYSTEM_SSH_CONFIG_D`, read-only and read by
    /// ssh in place of the host's.
    pub fn with_ssh_config(mut self, ssh_config: Option<PathBuf>) -> Self {
        self.ssh_config = ssh_config;
        self
    }

    pub fn ssh_config(&self) -> Option<&Path> {
        self.ssh_config.as_deref()
    }

    /// Every path a command may write: the declared set, and the agent's record of hosts when
    /// the launch forwards one.
    pub fn granted(&self) -> Vec<&Path> {
        let mut granted: Vec<&Path> = self.writable.paths().iter().map(PathBuf::as_path).collect();
        granted.extend(self.known_hosts.as_deref());
        granted
    }

    pub(crate) fn with_temp(&self, temp: &Path) -> Result<Self, BoundaryError> {
        let mut boundary = self.clone();
        if !self
            .writable
            .paths()
            .iter()
            .any(|path| temp.starts_with(path))
        {
            let mut paths = self.writable.paths().to_vec();
            paths.push(temp.to_owned());
            boundary.writable = WritableSet::new(paths)?;
        }
        Ok(boundary)
    }

    pub fn writable(&self) -> &WritableSet {
        &self.writable
    }

    pub fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }

    pub fn command_budget_ms(&self) -> u64 {
        self.command_budget_ms
    }

    pub fn output_budget_bytes(&self) -> u64 {
        self.output_budget_bytes
    }

    pub fn enforcement(&self) -> Enforcement {
        self.enforcement
    }

    pub fn description(&self) -> String {
        let mut lines: Vec<String> =
            vec![String::from("Run a command with `bash`."), String::new()];

        match self.enforcement {
            Enforcement::Bubblewrap => {
                lines.push(String::from(
                    "Commands run in unprivileged bubblewrap: the filesystem is read-only except \
                     the writable paths below.",
                ));
                lines.push(String::new());
                for path in self.granted() {
                    lines.push(format!("  {}", path.display()));
                }
                lines.push(String::new());
            }
            Enforcement::Unsafe => {
                lines.push(String::from(
                    "Unsafe: this run is not confined. Commands run with your own privileges, \
                     outside bubblewrap, and nothing limits what they may write.",
                ));
                lines.push(String::new());
            }
        }

        lines.push(format!(
            "The per-call timeout is {}s; a longer call is killed and reports `timed_out`.",
            self.timeout_seconds
        ));
        lines.push(format!(
            "The run's budgets are {}ms of command wall clock and {} bytes of command output.",
            self.command_budget_ms, self.output_budget_bytes
        ));

        lines.join("\n")
    }

    pub fn record_policy(&self) -> PolicyRecord {
        PolicyRecord {
            enforcement: self.enforcement,
            writable: self
                .granted()
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            timeout_seconds: self.timeout_seconds,
            command_budget_ms: self.command_budget_ms,
            output_budget_bytes: self.output_budget_bytes,
            description: self.description(),
        }
    }
}

/// Field order is fixed, so the JSON two runs write for one boundary is the same JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PolicyRecord {
    pub enforcement: Enforcement,
    pub writable: Vec<String>,
    pub timeout_seconds: u64,
    pub command_budget_ms: u64,
    pub output_budget_bytes: u64,
    pub description: String,
}

/// The `known_hosts` a forwarded agent appends to, when the launch asked for an agent and the file
/// already exists: a run records a host it meets and creates nothing. ssh expands `~` from the
/// passwd entry, so a `HOME` naming another directory names a file ssh will not write.
pub fn agent_known_hosts(config: &Config, environment: &impl Environment) -> Option<PathBuf> {
    if config.ssh_agent != Some(true) {
        return None;
    }
    let home = crate::config::spoken(environment, "HOME")?;
    if !Path::new(&home).is_absolute() {
        return None;
    }
    let known_hosts =
        std::fs::canonicalize(Path::new(&home).join(".ssh").join("known_hosts")).ok()?;
    known_hosts.is_file().then_some(known_hosts)
}

/// The host's ssh config drop-ins copied where ssh will accept them, when the launch asked for
/// an agent. ssh refuses a config file reached through `Include` unless root or the invoking
/// user owns it, and inside the sandbox's single-mapping user namespace root reads as `nobody`.
pub fn agent_ssh_config(config: &Config, drop_ins: &Path, temp: &Path) -> Option<PathBuf> {
    if config.ssh_agent != Some(true) {
        return None;
    }
    let staged = temp.join("ssh_config.d");
    std::fs::create_dir_all(&staged).ok()?;
    let drop_ins: Vec<PathBuf> = std::fs::read_dir(drop_ins)
        .ok()?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.extension().is_some_and(|kind| kind == "conf") && path.is_file())
        .collect();
    for path in &drop_ins {
        let copy = staged.join(path.file_name()?);
        std::fs::copy(path, &copy).ok()?;
        // A drop-in the host made group-writable would fail the same check on the far side.
        std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o644)).ok()?;
    }
    (!drop_ins.is_empty()).then_some(staged)
}

#[derive(Debug)]
pub enum BoundaryError {
    NotAbsolute { path: PathBuf },
    Unresolvable { path: PathBuf, reason: String },
    Root { path: PathBuf },
    Duplicate { path: PathBuf },
    Nested { path: PathBuf, inside: PathBuf },
    TimeoutAboveCeiling { seconds: u64, ceiling: u64 },
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoundaryError::NotAbsolute { path } => write!(
                formatter,
                "writable path {} is not absolute",
                path.display()
            ),
            BoundaryError::Unresolvable { path, reason } => write!(
                formatter,
                "writable path {} cannot be resolved: {reason}",
                path.display()
            ),
            BoundaryError::Root { path } => write!(
                formatter,
                "writable path {} is the root directory, which grants every path",
                path.display()
            ),
            BoundaryError::Duplicate { path } => {
                write!(formatter, "writable path {} appears twice", path.display())
            }
            BoundaryError::Nested { path, inside } => write!(
                formatter,
                "writable path {} is inside {}, which already grants it",
                path.display(),
                inside.display()
            ),
            BoundaryError::TimeoutAboveCeiling { seconds, ceiling } => write!(
                formatter,
                "per-call timeout {seconds}s is above the {ceiling}s ceiling"
            ),
        }
    }
}

impl Error for BoundaryError {}
