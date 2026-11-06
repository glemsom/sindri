//! Policy is the boundary in force, and every tool description derived from it:
//! the writable set, the per-call timeout and the budgets a run spends.
//!
//! One `Boundary` value holds all of it, so the four surfaces that show it — the
//! `bash` tool's description, the bytes `doctor` prints, the policy block a
//! session record stores and the sandbox fact the rail draws — are views of one
//! value rather than four copies free to drift. The description is rendered from
//! the fields it holds and from nothing else: never hand-written, never read out
//! of a file, so the boundary the model is told about is the boundary enforced.
//!
//! The writable set's invariants are the type's, not a convention: a set is
//! canonical, absolute, distinct, non-nested and never `/`, and a set that
//! breaks one is refused with a message naming the path that broke it.
//! Enforcement is two-valued, so a run is either in bubblewrap or unsafe and
//! there is no third thing any surface could say. The escape hatch is an
//! argument to the constructor and nothing else: it has no config key and no
//! environment variable, so it dies with the invocation that asked for it.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::Serialize;

use crate::config::Config;

/// The per-call timeout a run gets when the config names none, in seconds.
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 120;

/// The ceiling on the per-call timeout, in seconds: a longer value is refused
/// rather than clamped, because a clamp is a boundary nobody was told about.
pub const MAX_TIMEOUT_SECONDS: u64 = 3_600;

/// The command wall clock a run gets when the config names none, in
/// milliseconds.
pub const DEFAULT_COMMAND_BUDGET_MS: u64 = 3_600_000;

/// The command output a run gets when the config names none, in bytes.
pub const DEFAULT_OUTPUT_BUDGET_BYTES: u64 = 8 * 1024 * 1024;

/// How a run's commands are confined, as the two-valued fact it is: there is no
/// third state, so no surface can report one and the model cannot be told one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Enforcement {
    /// Unprivileged bubblewrap, with the writable set mapped writable.
    Bubblewrap,
    /// The `--yolo-unsafe` escape hatch: no bubblewrap at all.
    Unsafe,
}

impl Enforcement {
    /// The word the record and the JSON use, which is the word the spec uses.
    pub fn word(self) -> &'static str {
        match self {
            Enforcement::Bubblewrap => "bubblewrap",
            Enforcement::Unsafe => "unsafe",
        }
    }

    /// The word the rail uses, where the fact shares a ledger's narrow column.
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

/// The paths a run may write: canonical, absolute, distinct, non-nested and
/// never `/`, held in one sorted order so two routes to the same set — a file, a
/// variable, a flag — render one description rather than two spellings of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WritableSet {
    /// The canonical paths, sorted, so the order a caller named them in cannot
    /// reach the model.
    paths: Vec<PathBuf>,
}

impl WritableSet {
    /// Reads a set of paths, resolving each one through the filesystem, and
    /// refuses the set — naming the path that broke it — when any invariant
    /// fails.
    ///
    /// Canonical is read against the filesystem, not as string tidying: a path
    /// is resolved through its symlinks and its `..`, so two spellings of one
    /// directory are one directory. A path that cannot be resolved does not
    /// exist, and a run cannot be granted a directory that is not there.
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

        // Sorting is what makes the order of the routes irrelevant, and it is
        // also what lets both later checks read adjacent pairs only: a duplicate
        // lands beside its twin, and a path's ancestor always sorts before it.
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

    /// The paths, in the one order the set holds them.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

/// The boundary in force for a run: what may be written, how long a call may
/// take, what the run may spend in total, and how its commands are confined.
///
/// Every surface reads this value and nothing restates it, which is what makes
/// the model's description, `doctor`'s output and the record agree by
/// construction instead of by review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Boundary {
    /// The paths a run may write.
    writable: WritableSet,
    /// The per-call timeout, in seconds.
    timeout_seconds: u64,
    /// The run's command wall clock, in milliseconds.
    command_budget_ms: u64,
    /// The run's command output, in bytes.
    output_budget_bytes: u64,
    /// How the run's commands are confined.
    enforcement: Enforcement,
}

impl Boundary {
    /// The boundary a run holds: the workspace the run stands in plus every
    /// extra path the config named, with the limits the config sets and the
    /// documented defaults where it is silent.
    ///
    /// `enforcement` is the invocation's own fact. It is an argument because the
    /// escape hatch is not persistable — there is no key and no variable that
    /// could reach it — so a caller cannot arrive at `Unsafe` by configuration.
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

    /// The paths a run may write.
    pub fn writable(&self) -> &WritableSet {
        &self.writable
    }

    /// The per-call timeout, in seconds.
    pub fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }

    /// The run's command wall clock, in milliseconds.
    pub fn command_budget_ms(&self) -> u64 {
        self.command_budget_ms
    }

    /// The run's command output, in bytes.
    pub fn output_budget_bytes(&self) -> u64 {
        self.output_budget_bytes
    }

    /// How the run's commands are confined.
    pub fn enforcement(&self) -> Enforcement {
        self.enforcement
    }

    /// The `bash` tool's description: the boundary in force, rendered from the
    /// fields this value holds and from nothing else.
    ///
    /// Nothing here is read from the clock, the environment or a hash map, so
    /// two runs of one boundary render one string, byte for byte.
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
                for path in self.writable.paths() {
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

    /// The bytes `doctor` prints: the description the model sees, because a
    /// diagnostic that described the boundary differently would be the drift
    /// this value exists to prevent.
    pub fn doctor_description(&self) -> String {
        self.description()
    }

    /// The policy block a session record stores: the same boundary in the same
    /// words, including the description bytes the model was given verbatim.
    pub fn record_policy(&self) -> PolicyRecord {
        PolicyRecord {
            enforcement: self.enforcement,
            writable: self
                .writable
                .paths()
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

/// The policy block of a session record: the boundary a run held, in the shape a
/// later reader — `sessions show`, `jq`, the rail — reads it back from.
///
/// Field order is fixed, so the JSON two runs write for one boundary is the same
/// JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PolicyRecord {
    /// How the run's commands were confined.
    pub enforcement: Enforcement,
    /// The writable set, as the filesystem spells it.
    pub writable: Vec<String>,
    /// The per-call timeout, in seconds.
    pub timeout_seconds: u64,
    /// The run's command wall clock, in milliseconds.
    pub command_budget_ms: u64,
    /// The run's command output, in bytes.
    pub output_budget_bytes: u64,
    /// The bytes the model was given, verbatim.
    pub description: String,
}

/// Why a boundary cannot be held. Every one of them names the thing that broke
/// it, so a run that refuses a policy says which path or which limit to fix.
#[derive(Debug)]
pub enum BoundaryError {
    /// A path that is not absolute, which grants nothing because it means
    /// nothing without a directory to be relative to.
    NotAbsolute {
        /// The path as it was given.
        path: PathBuf,
    },
    /// A path the filesystem will not resolve, so there is no directory to
    /// grant.
    Unresolvable {
        /// The path as it was given.
        path: PathBuf,
        /// Why it would not resolve.
        reason: String,
    },
    /// The root directory, which as a writable path is every path.
    Root {
        /// The path, which is `/`.
        path: PathBuf,
    },
    /// One path given twice, which is a set that is not a set.
    Duplicate {
        /// The path that appeared twice.
        path: PathBuf,
    },
    /// A path inside another, which grants nothing the outer one does not.
    Nested {
        /// The inner path, which is the one to drop.
        path: PathBuf,
        /// The path that already contains it.
        inside: PathBuf,
    },
    /// A per-call timeout above the ceiling, refused rather than clamped.
    TimeoutAboveCeiling {
        /// The timeout the config asked for, in seconds.
        seconds: u64,
        /// The largest timeout a run may be given, in seconds.
        ceiling: u64,
    },
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
