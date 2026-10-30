//! The environment probe behind `sindri doctor`: prove bubblewrap, the required
//! binaries and the mount plan work before any model is in the loop.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths;
use crate::sandbox::{Enforcement, PolicyError, SandboxPolicy};
use crate::session::Env;

/// The binaries the system prompt may claim exist; `doctor` resolves each one
/// through the same `PATH` the sandbox will use. `bwrap` itself is checked
/// separately, because without it there is no sandbox to check anything in.
pub const REQUIRED_BINARIES: &[&str] = &[
    "bash", "rg", "curl", "lynx", "patch", "python3", "git", "jq", "yq", "xdg-open",
];

/// The marker a working sandbox prints back; its absence means the sandbox
/// never started.
const MARKER: &str = "sindri-doctor-sandbox-ok";

/// What a successful probe learned.
#[derive(Debug)]
pub struct Report {
    /// Exactly the text the model will be given, and what doctor prints.
    pub description: String,
}

/// Why the sandbox cannot exist on this machine.
#[derive(Debug)]
pub enum DoctorError {
    /// bwrap is not installed where the sandbox would look for it.
    BwrapMissing,
    /// bwrap is present but cannot build a namespace here.
    SandboxNotStarting { detail: String },
    /// Binaries the system prompt promises are missing from the sandbox path.
    BinariesMissing { missing: Vec<String> },
    /// The probe could not be set up at all.
    Setup(io::Error),
    /// The policy refused the paths this machine offers.
    Policy(PolicyError),
}

impl fmt::Display for DoctorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoctorError::BwrapMissing => write!(
                f,
                "bwrap not found: install bubblewrap (Debian/Ubuntu: sudo apt install bubblewrap) and re-run `sindri doctor`"
            ),
            DoctorError::SandboxNotStarting { detail } => write!(
                f,
                "bwrap cannot create a sandbox here: {detail} — allow unprivileged user namespaces (Ubuntu 24.04: sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0)"
            ),
            DoctorError::BinariesMissing { missing } => write!(
                f,
                "required binaries missing from the sandbox path: {} — install them and re-run `sindri doctor`",
                missing.join(", ")
            ),
            DoctorError::Setup(source) => write!(f, "cannot set up the probe: {source}"),
            DoctorError::Policy(source) => write!(f, "cannot build the sandbox policy: {source}"),
        }
    }
}

impl std::error::Error for DoctorError {}

/// Proves the sandbox works and returns the description the user will see.
///
/// The probe runs in a scratch directory of its own — not in the session
/// namespace — and removes it afterwards, so `doctor` leaves no record behind
/// and prunes nothing that a run wrote (ADR-0006).
///
/// The environment is taken as a value, the one a run would be opened in
/// (ADR-0015 §6), so the description is derived from the same workspace, `PATH`
/// and state root the run gets rather than from a second reading of the process.
pub fn probe(env: &Env, extras: &[PathBuf]) -> Result<Report, DoctorError> {
    let bwrap = which("bwrap").ok_or(DoctorError::BwrapMissing)?;
    let bash = which("bash").ok_or_else(|| DoctorError::BinariesMissing {
        missing: vec!["bash".to_string()],
    })?;

    let state = env.state.clone();
    let scratch = state
        .join("doctor")
        .join(paths::new_session_id().map_err(DoctorError::Setup)?);
    let session_temp = scratch.join("tmp");
    std::fs::create_dir_all(session_temp.join("home")).map_err(DoctorError::Setup)?;

    let probed = run_probe(&bwrap, &bash, env, extras, &session_temp);
    let _ = std::fs::remove_dir_all(&scratch);

    probed?;

    // The description states the shape a run gets, named by its session GUID.
    let shown_temp = state.join("sessions").join("<session>").join("tmp");
    let policy = build_policy(env, extras, shown_temp)?;
    Ok(Report {
        description: policy.describe(),
    })
}

fn build_policy(
    env: &Env,
    extras: &[PathBuf],
    session_temp: PathBuf,
) -> Result<SandboxPolicy, DoctorError> {
    crate::session::policy_for(
        &env.workspace,
        extras,
        session_temp,
        env.search_path.clone(),
        Enforcement::Bubblewrap,
    )
    .map_err(DoctorError::Policy)
}

fn run_probe(
    bwrap: &Path,
    bash: &Path,
    env: &Env,
    extras: &[PathBuf],
    session_temp: &Path,
) -> Result<(), DoctorError> {
    let policy = build_policy(env, extras, session_temp.to_path_buf())?;
    let script = format!(
        "echo {MARKER}; for b in {}; do command -v \"$b\" >/dev/null 2>&1 || echo \"missing:$b\"; done",
        REQUIRED_BINARIES.join(" ")
    );
    let command = vec![bash.display().to_string(), "-c".to_string(), script];
    let invocation = policy.to_argv(&command, None);

    // `_` keeps the resolved bwrap only as the reason the check exists; the
    // invocation itself is built from the argv, never assembled by hand.
    let _ = bwrap;
    let argv = invocation.argv();
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| DoctorError::Setup(io::Error::other("the sandbox argv is empty")))?;

    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(DoctorError::Setup)?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !stdout.contains(MARKER) {
        let detail = stderr
            .lines()
            .next()
            .unwrap_or("no output")
            .trim()
            .to_string();
        return Err(DoctorError::SandboxNotStarting { detail });
    }

    let missing: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("missing:"))
        .map(str::to_string)
        .collect();
    if !missing.is_empty() {
        return Err(DoctorError::BinariesMissing { missing });
    }
    Ok(())
}

/// Resolves a binary the way the shell would.
fn which(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| {
            candidate.is_file()
                && std::fs::metadata(candidate)
                    .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
        })
}
