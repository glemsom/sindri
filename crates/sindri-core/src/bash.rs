use std::error::Error;
use std::fmt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::pipe::Stream;
use crate::policy::{Boundary, BoundaryError, Enforcement};
use crate::run::Cancel;

pub const SHELL: &str = "/bin/sh";

pub const SANDBOX_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

pub const BWRAP: &str = "bwrap";

pub const TEMP_VARIABLE: &str = "TMPDIR";

const MAX_NAP: Duration = Duration::from_millis(25);

const SIGKILL: i32 = 9;

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
    let parent = std::process::id();
    let mut command = Command::new(&program);
    unsafe {
        command.pre_exec(move || {
            if prctl(1, SIGKILL, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if getppid() != parent as i32 {
                return Err(std::io::Error::other("bash parent ended before spawn"));
            }
            Ok(())
        });
    }
    let mut child = command
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

    let stdout = Stream::drain(child.stdout.take());
    let stderr = Stream::drain(child.stderr.take());

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
    unsafe {
        kill(pid, 19);
    }
    stop_descendants(pid);
    // The negative pid names the group. One the kernel already reaped has nothing left to
    // signal: a kill that lost a race, not a failure.
    unsafe {
        kill(-pid, SIGKILL);
    }
}

fn stop_descendants(parent: i32) {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    let mut children = Vec::new();
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((_, fields)) = stat.rsplit_once(") ") else {
            continue;
        };
        if fields
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<i32>().ok())
            == Some(parent)
        {
            unsafe {
                kill(pid, 19);
            }
            children.push(pid);
        }
    }
    for pid in children {
        unsafe {
            kill(pid, 19);
        }
        stop_descendants(pid);
        unsafe {
            kill(pid, SIGKILL);
        }
    }
}

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, arg2: i32, arg3: i32, arg4: i32, arg5: i32) -> i32;
    fn getppid() -> i32;
}

#[cfg(test)]
mod tests;
