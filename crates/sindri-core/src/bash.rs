//! The `bash` tool is the only way a run acts: one command string, run by a shell, confined by
//! the boundary in force. The network is kept on purpose, because search, fetch and HTTP run
//! through this shell: the boundary is on writes and not on reads.

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

pub const SHELL: &str = "/bin/sh";

pub const SANDBOX_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

pub const BWRAP: &str = "bwrap";

pub const TEMP_VARIABLE: &str = "TMPDIR";

/// Short enough that a call which ends at once is back at once, long enough that a long one
/// costs nothing.
const MAX_NAP: Duration = Duration::from_millis(25);

/// How long a reader gets to see the end of its pipe once the command has ended: a grandchild
/// left holding it cannot hold the call open.
const PIPE_GRACE: Duration = Duration::from_millis(250);

const SIGKILL: i32 = 9;

/// One argv, the program and its arguments. It has no public constructor, so the only route to a
/// command's argv is `invocation`: a call site cannot hand-assemble a boundary of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    program: String,
    args: Vec<String>,
}

impl Invocation {
    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn argv(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Outcome {
    pub enforcement: Enforcement,
    /// The exit code, or nothing when a signal killed it: left out of the JSON rather than
    /// written as `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum BashError {
    Boundary(BoundaryError),
    TempNotAbsolute { path: PathBuf },
    Process { program: String, reason: String },
}

impl BashError {
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

pub fn invocation(
    boundary: &Boundary,
    session_temp: &Path,
    command: &str,
) -> Result<Invocation, BashError> {
    let shell = Invocation {
        program: String::from(SHELL),
        args: vec![String::from("-c"), String::from(command)],
    };

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

    // `--die-with-parent` is what lets a timeout kill the whole sandbox, and `--new-session`
    // is what keeps it off the terminal.
    args.extend(
        [
            "--unshare-all",
            "--share-net",
            "--die-with-parent",
            "--new-session",
        ]
        .map(String::from),
    );

    args.extend(["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc"].map(String::from));

    for path in boundary.writable().paths() {
        let path = path.display().to_string();
        args.extend([String::from("--bind"), path.clone(), path]);
    }

    // The session temp is mapped and never created here: `session` makes it, adopts it and
    // clears it away.
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

    args.push(String::from("--"));
    args.push(shell.program);
    args.extend(shell.args);

    Ok(Invocation {
        program: String::from(BWRAP),
        args,
    })
}

pub fn run(
    boundary: &Boundary,
    session_temp: &Path,
    command: &str,
    cancel: &Cancel,
) -> Result<Outcome, BashError> {
    let invocation = invocation(boundary, session_temp, command)?;
    let program = invocation.program().to_owned();

    // No stdin: a command that reads one would hang the run. It leads a process group of its
    // own, so one kill takes everything it started.
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

/// Kills the process group outright, so no child a command started outlives the call. A
/// sandboxed command needs it too: a kill that reaches only the sandbox's edge leaves a
/// command still working inside it.
fn stop(child: &mut Child) {
    let Ok(pid) = i32::try_from(child.id()) else {
        let _ = child.kill();
        return;
    };
    // The negative pid names the group. One the kernel already reaped has nothing left to
    // signal: a kill that lost a race, not a failure.
    unsafe {
        kill(-pid, SIGKILL);
    }
}

unsafe extern "C" {
    /// `kill(2)`, declared here so the module names no crate to stop a command: one syscall that
    /// names a process group.
    fn kill(pid: i32, signal: i32) -> i32;
}

/// A stream read by a thread of its own, so a command that fills both cannot block on the one
/// nobody is reading, and collected into a sink the call can take whether or not the read ended.
struct Pipe {
    sink: Arc<Mutex<Vec<u8>>>,
    reader: JoinHandle<()>,
}

impl Pipe {
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

    /// Real directories, because the writable set is canonicalised through the filesystem.
    struct Ground {
        _root: TempDir,
        path: PathBuf,
    }

    impl Ground {
        fn new() -> Ground {
            let root = tempfile::tempdir().expect("a temp ground");
            let path = std::fs::canonicalize(root.path()).expect("the ground resolves");
            Ground { _root: root, path }
        }

        fn directory(&self, name: &str) -> PathBuf {
            let directory = self.path.join(name);
            std::fs::create_dir_all(&directory).expect("the directory");
            std::fs::canonicalize(&directory).expect("the directory resolves")
        }
    }

    /// `extra` is named in the order given, so a test can show the order does not reach the argv.
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

    /// Not on this machine on purpose: the argv is rendered from the value and never from what
    /// the filesystem says.
    const TEMP: &str = "/run/sindri/7f3c/tmp";

    #[test]
    fn the_sandbox_argv_is_the_boundary_and_nothing_else() {
        let ground = Ground::new();
        // Named in reverse, so the vector shows the boundary's sorted order, not the caller's.
        let boundary = boundary(&ground, &["zdata", "adata"], Enforcement::Bubblewrap);
        let workspace = ground.path.join("workspace");
        let adata = ground.path.join("adata");
        let zdata = ground.path.join("zdata");

        let argv = invocation(&boundary, Path::new(TEMP), "echo hi")
            .expect("the boundary renders an argv")
            .argv();

        // Written out by hand rather than built by the code under test, so the
        // builder cannot drift into saying otherwise.
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
