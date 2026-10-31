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

pub use sindri_harness as provider;

pub mod pty;
pub mod records;
pub mod screen;

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

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

    pub fn command(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin!("sindri"));
        cmd.current_dir(&self.workspace)
            .env("XDG_STATE_HOME", &self.state)
            .env("XDG_CONFIG_HOME", &self.config);
        // The host's own settings must not leak into a test's run, and that
        // includes what it says about its terminal: a frame that hears a
        // Kitty-capable terminal asks it a question, and the answer it gets is
        // not the pty's.
        for name in provider::PROVIDER_ENV {
            cmd.env_remove(name);
        }
        for name in TERMINAL_ENV {
            cmd.env_remove(name);
        }
        cmd
    }

    pub fn cmd(&self) -> Command {
        Command::from_std(self.command())
    }

    pub fn canonical_workspace(&self) -> PathBuf {
        self.workspace.canonicalize().expect("canonical workspace")
    }
}

/// What the host says about its terminal, so a test's pty is the only terminal
/// a run can hear.
pub const TERMINAL_ENV: &[&str] = &[
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "LC_TERMINAL",
    "KITTY_WINDOW_ID",
    "COLORTERM",
];

pub fn write_config(env: &Env, toml: &str) {
    let dir = env.config.join("sindri");
    std::fs::create_dir_all(&dir).expect("config dir");
    std::fs::write(dir.join("config.toml"), toml).expect("write config");
}

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

pub fn signal(pid: u32, signal: i32) {
    let pid = libc::pid_t::try_from(pid).expect("a pid fits in a pid_t");
    // SAFETY: `kill` is called with a pid the caller just spawned and owns;
    // a failure means the process is already gone, which the caller's own
    // assertions will report.
    let sent = unsafe { libc::kill(pid, signal) };
    assert_eq!(sent, 0, "signal {signal} to {pid}");
}

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

pub const TEST_KEY: &str = "sk-ant-test-not-a-real-key";

pub fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

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

pub fn resolve(name: &str, path: &str) -> Option<PathBuf> {
    path.split(':')
        .map(|dir| Path::new(dir).join(name))
        .find(|candidate| candidate.is_file())
}

pub fn host_path() -> String {
    std::env::var("PATH").expect("PATH")
}

/// Every file under `dir`, as one string: a credential must not be in any of
/// them, and a record is all of them together.
pub fn everything_written(dir: &Path) -> String {
    let mut all = String::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                all.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    all
}

/// The session directories a run left under this environment's state root,
/// sorted: one directory per record, and the name is the session id.
pub fn sessions(env: &Env) -> Vec<PathBuf> {
    let root = env.state.join("sindri/sessions");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&root)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    found.sort();
    found
}

/// The one session directory this environment holds: a test that expects a
/// single run, and says so rather than picking from a list.
pub fn the_session(env: &Env) -> PathBuf {
    let found = sessions(env);
    assert_eq!(found.len(), 1, "one run, one record: {found:?}");
    found.into_iter().next().expect("the session directory")
}
