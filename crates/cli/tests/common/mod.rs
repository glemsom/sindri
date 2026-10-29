//! Shared harness for the process seam: every test runs the real `sindri`
//! binary as a subprocess with its own `XDG_*` directories and workspace.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]
#![allow(
    dead_code,
    reason = "each test binary uses a different slice of the harness"
)]

/// The mock provider, from the harness crate every process-seam test drives.
pub use sindri_harness as provider;

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// An isolated host: a workspace to run in and the state and config
/// directories a real user would have.
pub struct Env {
    _root: TempDir,
    pub workspace: PathBuf,
    pub state: PathBuf,
    pub config: PathBuf,
}

impl Env {
    pub fn new() -> Self {
        let root = TempDir::new().expect("temp dir");
        let workspace = root.path().join("workspace");
        let state = root.path().join("state");
        let config = root.path().join("config");
        for dir in [&workspace, &state, &config] {
            std::fs::create_dir_all(dir).expect("create dir");
        }
        Env {
            _root: root,
            workspace,
            state,
            config,
        }
    }

    /// The `sindri` binary with this environment applied and the workspace as
    /// the current directory, for a test that must spawn it itself.
    pub fn command(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("sindri"));
        cmd.current_dir(&self.workspace)
            .env("XDG_STATE_HOME", &self.state)
            .env("XDG_CONFIG_HOME", &self.config);
        // The host's own settings must not leak into a test's run.
        for name in provider::PROVIDER_ENV {
            cmd.env_remove(name);
        }
        cmd
    }

    /// The same command, for assertions.
    pub fn cmd(&self) -> Command {
        Command::from_std(self.command())
    }

    /// The workspace as the agent will see it: canonical.
    pub fn canonical_workspace(&self) -> PathBuf {
        self.workspace.canonicalize().expect("canonical workspace")
    }
}

/// Writes `$XDG_CONFIG_HOME/sindri/config.toml`.
pub fn write_config(env: &Env, toml: &str) {
    let dir = env.config.join("sindri");
    std::fs::create_dir_all(&dir).expect("config dir");
    std::fs::write(dir.join("config.toml"), toml).expect("write config");
}

/// Runs batch mode with exactly the settings the test supplies, so each test
/// states its own precedence rather than inheriting the harness's.
pub fn run_with(env: &Env, args: &[&str], vars: &[(&str, &str)]) -> std::process::Output {
    env.cmd()
        .args(args)
        .env("SINDRI_API_KEY", TEST_KEY)
        .envs(vars.iter().copied())
        .write_stdin("")
        .assert()
        .get_output()
        .clone()
}

/// Starts batch mode as a live child, so a test can signal it mid-run. Its
/// stdin is closed, its streams are pipes, and the mock is its provider.
pub fn spawn(env: &Env, mock: &provider::MockProvider, args: &[&str]) -> std::process::Child {
    env.command()
        .args(args)
        .env("SINDRI_API_KEY", TEST_KEY)
        .env("SINDRI_MODEL", "claude-sonnet-4-6")
        .env("SINDRI_BASE_URL", mock.base_url())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn sindri")
}

/// Sends `signal` to `pid`, as a shell would on Ctrl-C or `kill -9`.
pub fn signal(pid: u32, signal: i32) {
    let pid = libc::pid_t::try_from(pid).expect("a pid fits in a pid_t");
    // SAFETY: `kill` is called with a pid the caller just spawned and owns;
    // a failure means the process is already gone, which the caller's own
    // assertions will report.
    let sent = unsafe { libc::kill(pid, signal) };
    assert_eq!(sent, 0, "signal {signal} to {pid}");
}

/// Waits until `path` exists, then returns. A marker that never appears is the
/// failure: the thing the test meant to interrupt never started.
pub fn wait_until(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("{} never appeared", path.display());
}

/// A credential shaped like a real one but belonging to nothing.
pub const TEST_KEY: &str = "sk-ant-test-not-a-real-key";

/// Everything a process wrote.
pub fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Everything a process complained about.
pub fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Runs batch mode against `mock`, with a credential, a model and the mock's
/// base URL supplied through the environment.
pub fn batch(
    env: &Env,
    mock: &provider::MockProvider,
    args: &[&str],
    stdin: &str,
) -> std::process::Output {
    let mut cmd = env.cmd();
    cmd.args(args)
        .env("SINDRI_API_KEY", TEST_KEY)
        .env("SINDRI_MODEL", "claude-sonnet-4-6")
        .env("SINDRI_BASE_URL", mock.base_url())
        .write_stdin(stdin.to_string())
        .assert()
        .get_output()
        .clone()
}

/// Every binary the sandbox must be able to resolve: the required set, plus
/// bwrap itself, which `doctor` checks before anything else.
pub fn required_binaries() -> Vec<&'static str> {
    let mut names = sindri_core::doctor::REQUIRED_BINARIES.to_vec();
    names.push("bwrap");
    names
}

/// A directory holding symlinks to every binary in `names` that resolves on
/// `path`, minus `omit`. Writing this directory onto `PATH` hides one binary
/// without touching the host's real tools.
pub fn path_without(names: &[&str], omit: &[&str], path: &str) -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    for name in names {
        if omit.contains(name) {
            continue;
        }
        if let Some(candidate) = resolve(name, path) {
            std::os::unix::fs::symlink(&candidate, dir.path().join(name)).expect("symlink binary");
        }
    }
    dir
}

/// The first executable named `name` on `path`, the way a shell would find it.
pub fn resolve(name: &str, path: &str) -> Option<PathBuf> {
    path.split(':')
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| candidate.is_file())
}

/// The host's own `PATH`.
pub fn host_path() -> String {
    std::env::var("PATH").expect("PATH")
}
