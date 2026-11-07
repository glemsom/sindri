//! The `bash` tool is the only way a run acts: one command string, run by a
//! shell, confined by the boundary in force, its output and its verdict brought
//! back as one value.
//!
//! [`invocation`] is how a boundary becomes an argv, and it is a pure function:
//! the `Boundary` — the writable set, the enforcement — and a session temp handed
//! in, and out comes the process that expresses exactly that boundary. No call
//! site assembles a flag of its own, so what confines a command cannot drift from
//! the policy the model was told.
//!
//! A sandboxed command runs in unprivileged bubblewrap: the host root read-only,
//! the writable set bound over itself where the model was told it is, the session
//! temp bound and mapped to `TMPDIR`, and the network kept, because §5 sends
//! search, fetch and HTTP through this shell — the boundary is one on writes and
//! not on reads. Its environment is cleared down to that `TMPDIR` and a fixed
//! `PATH`, so a credential in this process's environment cannot reach a command
//! that could send it anywhere. An `unsafe` run is the other half of the
//! two-valued fact: nothing between it and the user's own privileges, inheriting
//! environment and cwd.
//!
//! [`run`] spawns that argv and waits, until the per-call timeout expires, when
//! it kills the command and the outcome says `timed_out` — `--die-with-parent`
//! being what takes the whole sandbox with it — or until the user's word arrives,
//! when it kills the process group the command leads and the outcome says
//! `cancelled` (§4).
//!
//! The session temp is an input and not a thing this module makes:
//! [`crate::session`] creates it, adopts it and clears it away, and nothing here
//! creates, adopts or clears it a second time.
//! The run's budgets and the record are not here either — an outcome is what a
//! budget is spent from and what a record is written about.

use std::error::Error;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::policy::{Boundary, BoundaryError, Enforcement};
use crate::run::Cancel;

/// The shell every command string is handed to.
pub const SHELL: &str = "/bin/sh";

/// The `PATH` a sandboxed command gets, fixed so two calls see one shell.
pub const SANDBOX_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

/// The program that creates the sandbox.
pub const BWRAP: &str = "bwrap";

/// The variable the session temp is mapped to inside the sandbox.
pub const TEMP_VARIABLE: &str = "TMPDIR";

/// The longest the wait naps between polls: short enough that a call which ends
/// at once is back at once, long enough that a long one costs nothing.
const MAX_NAP: Duration = Duration::from_millis(25);

/// How long a reader gets to see the end of its pipe once the command has
/// ended, so a grandchild left holding the pipe cannot hold the call open.
const PIPE_GRACE: Duration = Duration::from_millis(250);

/// The signal a stopped command is stopped with: no code to report, and no way
/// for the command to have a last word about it.
const SIGKILL: i32 = 9;

/// One argv: the program and the arguments, in the order they are spawned.
///
/// It has no public constructor, so the only route to a command's argv is
/// [`invocation`]: a call site cannot hand-assemble a boundary of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// The program to spawn.
    program: String,
    /// Its arguments.
    args: Vec<String>,
}

impl Invocation {
    /// The program to spawn.
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Its arguments, in the order they are spawned.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// The whole argv, program first.
    pub fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }
}

/// What one command did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// How the command was confined, so the JSON and the rail say `unsafe`.
    pub enforcement: Enforcement,
    /// The exit code, or nothing when a signal killed the command. Left out of
    /// the JSON rather than written as `null` (spec §15).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    /// Whether the per-call timeout killed the command.
    pub timed_out: bool,
    /// Whether the user's word stopped the command while it ran (§4).
    pub cancelled: bool,
    /// What the command wrote to stdout.
    pub stdout: String,
    /// What the command wrote to stderr.
    pub stderr: String,
}

/// Why a command could not be run.
#[derive(Debug)]
pub enum BashError {
    /// The boundary cannot be held, which is a usage error rather than a
    /// best effort.
    Boundary(BoundaryError),
    /// The session temp is not absolute.
    TempNotAbsolute {
        /// The path as it was given.
        path: PathBuf,
    },
    /// The command could not be started or waited for.
    Process {
        /// The program that would not run.
        program: String,
        /// Why it would not.
        reason: String,
    },
}

impl BashError {
    /// The exit code a front end reports.
    pub fn exit_code(&self) -> u8 {
        match self {
            BashError::Boundary(_) | BashError::TempNotAbsolute { .. } => 2,
            BashError::Process { .. } => 3,
        }
    }
}

impl fmt::Display for BashError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BashError::Boundary(error) => write!(formatter, "{error}"),
            BashError::TempNotAbsolute { path } => {
                write!(formatter, "session temp {} is not absolute", path.display())
            }
            BashError::Process { program, reason } => {
                write!(formatter, "cannot run {program}: {reason}")
            }
        }
    }
}

impl Error for BashError {}

impl From<BoundaryError> for BashError {
    fn from(error: BoundaryError) -> BashError {
        BashError::Boundary(error)
    }
}

/// The argv that expresses `boundary` for one command: the boundary turned into
/// the process that enforces it, so no call site assembles a flag of its own.
///
/// It is a pure function — the boundary, the session temp and the command in, an
/// argv out — which is what lets the whole confinement be read and reviewed as a
/// value, and checked against golden vectors with no kernel involved.
pub fn invocation(
    boundary: &Boundary,
    session_temp: &Path,
    command: &str,
) -> Result<Invocation, BashError> {
    // The command as the model wrote it: one string, handed to a shell, because
    // a pipeline or a short script is one turn's work.
    let shell = Invocation {
        program: String::from(SHELL),
        args: vec![String::from("-c"), String::from(command)],
    };

    // The escape hatch is nothing between the run and the user's own
    // privileges: no bubblewrap, no mounts, no environment of ours, and the cwd
    // the process already had. The temp is not mapped, because nothing is.
    if boundary.enforcement() == Enforcement::Unsafe {
        return Ok(shell);
    }

    if !session_temp.is_absolute() {
        return Err(BashError::TempNotAbsolute {
            path: session_temp.to_path_buf(),
        });
    }

    let temp = session_temp.display().to_string();
    let mut args: Vec<String> = Vec::new();

    // Every namespace unshared, the network kept: §5 sends search, fetch and
    // HTTP through this shell, so the boundary is a boundary on writes and not
    // on reads. Dying with the parent is what makes the timeout a kill of the
    // whole sandbox rather than of one process in it, and the new session keeps
    // a sandboxed command off the terminal that started it.
    args.extend(
        [
            "--unshare-all",
            "--share-net",
            "--die-with-parent",
            "--new-session",
        ]
        .map(String::from),
    );

    // The host root, read-only: the command sees the machine it always saw and
    // may write none of it. /dev and /proc are mounted over it so a command has
    // the devices and the process view a shell expects.
    args.extend(["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc"].map(String::from));

    // The writable set, exactly as the boundary holds it: canonical, sorted,
    // non-nested. Each path is bound over itself, so it stays where the model
    // was told it is.
    for path in boundary.writable().paths() {
        let path = path.display().to_string();
        args.extend([String::from("--bind"), path.clone(), path]);
    }

    // The session temp, mapped to TMPDIR: scratch that lives as long as the
    // session. This module is handed the path and maps it; it does not create
    // it, adopt it or clear it away.
    args.extend([
        String::from("--bind"),
        temp.clone(),
        temp.clone(),
        String::from("--clearenv"),
        String::from("--setenv"),
        String::from(TEMP_VARIABLE),
        temp,
        String::from("--setenv"),
        String::from("PATH"),
        String::from(SANDBOX_PATH),
    ]);

    // The separator, and then the command the model wrote.
    args.push(String::from("--"));
    args.push(shell.program);
    args.extend(shell.args);

    Ok(Invocation {
        program: String::from(BWRAP),
        args,
    })
}

/// Runs one command under `boundary` and brings back what it did, until it ends
/// or until the user's word arrives on `cancel`.
///
/// The argv comes from [`invocation`] and from nowhere else, so what confines the
/// command is the boundary and not this function's own opinion of it. The call is
/// waited for until the per-call timeout expires, at which point the command is
/// killed — bubblewrap's `--die-with-parent` takes the whole sandbox with it — and
/// the outcome says `timed_out`. A cancellation is the other kill and the louder
/// one: the command's whole process group goes, so an unsafe run's own children
/// stop with it, and the outcome says `cancelled` (§4, ADR-0006).
pub fn run(
    boundary: &Boundary,
    session_temp: &Path,
    command: &str,
    cancel: &Cancel,
) -> Result<Outcome, BashError> {
    let invocation = invocation(boundary, session_temp, command)?;
    let program = invocation.program().to_owned();

    // No stdin: a tool call has no terminal, and a command that reads one would
    // hang the run. Both streams are piped, because the model is given them.
    //
    // The command leads a process group of its own, so one kill takes everything
    // it started: `--die-with-parent` is what carries a sandboxed command's
    // process with it, and the group is what carries an unsafe one's children.
    let mut child = Command::new(&program)
        .args(invocation.args())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| BashError::Process {
            program: program.clone(),
            reason: error.to_string(),
        })?;

    let stdout = Pipe::drain(child.stdout.take());
    let stderr = Pipe::drain(child.stderr.take());

    let deadline = Instant::now() + Duration::from_secs(boundary.timeout_seconds());
    let mut nap = Duration::from_millis(1);
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(error) => {
                stop(&mut child);
                let _ = child.wait();
                return Err(BashError::Process {
                    program,
                    reason: format!("waiting for it failed: {error}"),
                });
            }
        }
        // The user's word first: a cancellation beats the timeout that would
        // otherwise have fired, because it is the louder of the two (§4,
        // ADR-0006).
        if cancel.is_set() {
            cancelled = true;
            stop(&mut child);
            break child.wait().ok();
        }
        if Instant::now() >= deadline {
            timed_out = true;
            stop(&mut child);
            break child.wait().ok();
        }
        thread::sleep(nap);
        nap = (nap * 2).min(MAX_NAP);
    };

    Ok(Outcome {
        enforcement: boundary.enforcement(),
        code: status.and_then(|status| status.code()),
        timed_out,
        cancelled,
        stdout: String::from_utf8_lossy(&stdout.finish()).into_owned(),
        stderr: String::from_utf8_lossy(&stderr.finish()).into_owned(),
    })
}

/// Stops a command and everything it started: the process group it leads, killed
/// outright, so no child an unsafe run's own command started outlives the call
/// that made it (§4). A sandboxed command needs it too — a kill that reaches
/// only the sandbox's edge is a command still working inside it.
fn stop(child: &mut Child) {
    let Ok(pid) = i32::try_from(child.id()) else {
        let _ = child.kill();
        return;
    };
    // The negative pid names the group, the command leading it included. A group
    // the kernel has already reaped has nothing left to signal, which is a kill
    // that lost a race and not a failure.
    unsafe {
        kill(-pid, SIGKILL);
    }
}

unsafe extern "C" {
    /// `kill(2)`, declared here as `run` declares `signal(2)`: one syscall that
    /// names a process group, so the module keeps naming no crate to stop a
    /// command.
    fn kill(pid: i32, signal: i32) -> i32;
}

/// One of a command's output streams, read by a thread of its own so a command
/// that fills both cannot block on the one nobody is reading, and collected into
/// a sink the call can take whether or not the reader has finished.
struct Pipe {
    /// The bytes read so far.
    sink: Arc<Mutex<Vec<u8>>>,
    /// The thread reading them.
    reader: JoinHandle<()>,
}

impl Pipe {
    /// Starts reading `pipe` to its end.
    fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Pipe {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let collected = Arc::clone(&sink);
        let reader = thread::spawn(move || {
            let Some(mut pipe) = pipe else {
                return;
            };
            let mut chunk = [0u8; 8 * 1024];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => return,
                    Ok(read) => collected
                        .lock()
                        .expect("the sink is not poisoned")
                        .extend_from_slice(&chunk[..read]),
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => return,
                }
            }
        });
        Pipe { sink, reader }
    }

    /// The bytes read so far, after a grace for the reader to reach the end.
    fn finish(self) -> Vec<u8> {
        let deadline = Instant::now() + PIPE_GRACE;
        while !self.reader.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        self.sink.lock().expect("the sink is not poisoned").clone()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::config::Config;

    /// Real directories, because the writable set is canonicalised through the
    /// filesystem rather than tidied as a string.
    struct Ground {
        /// Held so the directories outlive the ground that names them.
        _root: TempDir,
        /// The same directory, as the filesystem spells it.
        path: PathBuf,
    }

    impl Ground {
        /// A ground of its own, canonical.
        fn new() -> Ground {
            let root = tempfile::tempdir().expect("a temp ground");
            let path = std::fs::canonicalize(root.path()).expect("the ground resolves");
            Ground { _root: root, path }
        }

        /// A directory under the ground, created and returned canonical.
        fn directory(&self, name: &str) -> PathBuf {
            let directory = self.path.join(name);
            std::fs::create_dir_all(&directory).expect("the directory");
            std::fs::canonicalize(&directory).expect("the directory resolves")
        }
    }

    /// A boundary over the ground's workspace plus `extra`, named in the order
    /// given so a test can show the order does not reach the argv.
    fn boundary(ground: &Ground, extra: &[&str], enforcement: Enforcement) -> Boundary {
        let workspace = ground.directory("workspace");
        let config = Config {
            extra_writable_paths: Some(
                extra
                    .iter()
                    .map(|name| ground.directory(name))
                    .collect::<Vec<PathBuf>>(),
            ),
            ..Config::default()
        };
        Boundary::from_config(&config, &workspace, enforcement).expect("the boundary holds")
    }

    /// A session temp that is not on this machine, which is the point: the argv
    /// is rendered from the value and never from what the filesystem says.
    const TEMP: &str = "/run/sindri/7f3c/tmp";

    #[test]
    fn the_sandbox_argv_is_the_boundary_and_nothing_else() {
        let ground = Ground::new();
        // Named in reverse, so the vector shows the boundary's sorted order
        // rather than the order a caller happened to name.
        let boundary = boundary(&ground, &["zdata", "adata"], Enforcement::Bubblewrap);
        let workspace = ground.path.join("workspace");
        let adata = ground.path.join("adata");
        let zdata = ground.path.join("zdata");

        let argv = invocation(&boundary, Path::new(TEMP), "echo hi")
            .expect("the boundary renders an argv")
            .argv();

        // The worked example, written out: an independent statement of what
        // confines a command, so the builder cannot drift into saying otherwise.
        let expected: Vec<String> = vec![
            "bwrap",
            "--unshare-all",
            "--share-net",
            "--die-with-parent",
            "--new-session",
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--bind",
            &adata.display().to_string(),
            &adata.display().to_string(),
            "--bind",
            &workspace.display().to_string(),
            &workspace.display().to_string(),
            "--bind",
            &zdata.display().to_string(),
            &zdata.display().to_string(),
            "--bind",
            TEMP,
            TEMP,
            "--clearenv",
            "--setenv",
            "TMPDIR",
            TEMP,
            "--setenv",
            "PATH",
            "/usr/local/bin:/usr/bin:/bin",
            "--",
            "/bin/sh",
            "-c",
            "echo hi",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        assert_eq!(argv, expected);
    }

    #[test]
    fn the_unsafe_argv_is_the_shell_alone() {
        let ground = Ground::new();
        let boundary = boundary(&ground, &["adata"], Enforcement::Unsafe);

        let argv = invocation(&boundary, Path::new(TEMP), "echo hi")
            .expect("the boundary renders an argv")
            .argv();

        // Nothing between the run and the user's own privileges: no bubblewrap,
        // no mounts, no environment of ours, and the cwd the process already has.
        assert_eq!(argv, vec!["/bin/sh", "-c", "echo hi"]);
    }

    #[test]
    fn a_relative_session_temp_is_a_usage_error() {
        let ground = Ground::new();
        let boundary = boundary(&ground, &[], Enforcement::Bubblewrap);

        let error = invocation(&boundary, Path::new("relative/tmp"), "echo hi")
            .expect_err("a relative temp names no directory a sandbox could mount");

        assert_eq!(
            error.exit_code(),
            2,
            "a bad call is a usage error, not a best effort"
        );
        assert!(
            error.to_string().contains("relative/tmp"),
            "the refusal names the path to fix: {error}"
        );
    }

    #[test]
    fn an_invalid_writable_set_is_a_usage_error_not_a_best_effort() {
        let ground = Ground::new();
        let workspace = ground.directory("workspace");
        let data = ground.directory("data");
        let inner = data.join("inner");
        std::fs::create_dir_all(&inner).expect("the inner directory");
        let (data_name, inner_name) = (data.display().to_string(), inner.display().to_string());

        // Every way a set stops being a set, each with the path that broke it.
        let broken: Vec<(&str, Vec<PathBuf>, &str)> = vec![
            ("duplicate", vec![data.clone(), data.clone()], &data_name),
            ("nested", vec![data.clone(), inner.clone()], &inner_name),
            ("root", vec![PathBuf::from("/")], "/"),
            ("relative", vec![PathBuf::from("data")], "data"),
        ];

        for (shape, paths, named) in broken {
            let config = Config {
                extra_writable_paths: Some(paths),
                ..Config::default()
            };
            let refused = Boundary::from_config(&config, &workspace, Enforcement::Bubblewrap)
                .expect_err(&format!("a {shape} writable set must be refused"));
            let error = BashError::from(refused);

            assert_eq!(
                error.exit_code(),
                2,
                "a {shape} writable set is a usage error, not a best effort"
            );
            assert!(
                error.to_string().contains(named),
                "the {shape} refusal names the path that broke it: {error}"
            );
        }
    }
}
