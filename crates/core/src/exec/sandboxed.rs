//! The real executor: bwrap exactly as the policy derives it, our own deadline,
//! and an outcome classified from bwrap's status channel rather than guessed at
//! (ADR-0007).

use std::fs::File;
use std::io::{self, Read};
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::BashExecutor;
use super::result::{Outcome, OutputLimit, ToolResult, truncate};
use super::status::{exit_code, exit_status_code, first_line};
use crate::sandbox::SandboxPolicy;

/// The deadline one command gets when nobody configured one.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
/// The longest deadline a user may ask for.
pub const MAX_TIMEOUT_SECS: u64 = 3600;

/// The fd bwrap writes its status documents to, inside the child.
const STATUS_FD: RawFd = 3;
/// How long a signalled process group gets before it is killed outright.
const KILL_GRACE: Duration = Duration::from_secs(2);
/// How often the deadline is re-checked.
const POLL: Duration = Duration::from_millis(10);

/// Runs `bash` scripts inside the sandbox described by one policy.
pub struct SandboxExecutor {
    policy: SandboxPolicy,
    timeout: Duration,
    spill_dir: PathBuf,
    limit: OutputLimit,
    /// Names each command's spill files, so turn n's path is still turn n's.
    calls: AtomicU64,
}

impl SandboxExecutor {
    /// Builds an executor for one session: its policy, its deadline, and where
    /// over-long output is spilled.
    pub fn new(policy: SandboxPolicy, timeout: Duration, spill_dir: PathBuf) -> Self {
        SandboxExecutor {
            policy,
            timeout,
            spill_dir,
            limit: OutputLimit::default(),
            calls: AtomicU64::new(0),
        }
    }
}

impl BashExecutor for SandboxExecutor {
    fn execute(&self, script: &str) -> ToolResult {
        let started = Instant::now();
        match self.run(script, started) {
            Ok(result) => result,
            Err(detail) => ToolResult {
                outcome: Outcome::SandboxError { detail },
                stdout: String::new(),
                stderr: String::new(),
                truncated: false,
                spill: Vec::new(),
                duration_ms: started.elapsed().as_millis() as u64,
            },
        }
    }
}

impl SandboxExecutor {
    fn run(&self, script: &str, started: Instant) -> Result<ToolResult, String> {
        let command = vec!["bash".to_string(), "-c".to_string(), script.to_string()];
        let sandboxed = !self.policy.is_unsafe();
        let invocation = self
            .policy
            .to_argv(&command, sandboxed.then_some(STATUS_FD));
        let argv = invocation.argv();

        let mut process = Command::new(&argv[0]);
        process
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);

        let mut status_pipe = None;
        if sandboxed {
            let (read_end, write_end) = pipe().map_err(|error| error.to_string())?;
            // SAFETY: `pipe()` returned two fresh, exclusively owned descriptors;
            // wrapping each exactly once means nothing double-closes them.
            let (writer, reader) =
                unsafe { (File::from_raw_fd(write_end), File::from_raw_fd(read_end)) };
            // SAFETY: `dup2` and `fcntl` are async-signal-safe and touch only the
            // child's own descriptor table; both descriptor numbers are captured
            // by value, so the closure owns nothing that could dangle.
            unsafe {
                process.pre_exec(move || {
                    if write_end == STATUS_FD {
                        // `dup2` onto itself would leave the close-on-exec flag in
                        // place, so clear it explicitly.
                        if libc::fcntl(STATUS_FD, libc::F_SETFD, 0) < 0 {
                            return Err(io::Error::last_os_error());
                        }
                    } else if libc::dup2(write_end, STATUS_FD) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            status_pipe = Some((writer, reader));
        }

        let mut child = process.spawn().map_err(|error| error.to_string())?;

        // Release our own write end so the reader can reach EOF, then drain all
        // three channels while the child runs.
        let status_docs = status_pipe.take().map(|(writer, reader)| {
            drop(writer);
            drain(reader)
        });
        let stdout = child.stdout.take().map(drain);
        let stderr = child.stderr.take().map(drain);

        let timed_out = wait_with_deadline(&mut child, self.timeout);
        let status = child.wait().map_err(|error| error.to_string())?;

        let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        let (stdout, stdout_trouble) = collect(stdout);
        let (stderr, stderr_trouble) = collect(stderr);
        let docs = collect(status_docs).0;

        let mut stdout_text = truncate(
            &stdout,
            self.limit,
            Some(&self.spill_dir.join(format!("{call}-stdout.txt"))),
        );
        let mut stderr_text = truncate(
            &stderr,
            self.limit,
            Some(&self.spill_dir.join(format!("{call}-stderr.txt"))),
        );

        // A stream we could not read is a stream we must not present as complete.
        for (cut, trouble) in [
            (&mut stdout_text, stdout_trouble),
            (&mut stderr_text, stderr_trouble),
        ] {
            if let Some(error) = trouble {
                cut.text.push_str(&format!("\noutput incomplete: {error}"));
                cut.truncated = true;
            }
        }

        Ok(ToolResult {
            outcome: self.classify(timed_out, &status, &docs, sandboxed, &stderr),
            stdout: stdout_text.text,
            stderr: stderr_text.text,
            truncated: stdout_text.truncated || stderr_text.truncated,
            spill: [stdout_text.spill, stderr_text.spill]
                .into_iter()
                .flatten()
                .collect(),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn classify(
        &self,
        timed_out: bool,
        status: &ExitStatus,
        docs: &[u8],
        sandboxed: bool,
        stderr: &[u8],
    ) -> Outcome {
        if timed_out {
            return Outcome::TimedOut;
        }
        if !sandboxed {
            return Outcome::Exited {
                exit_code: exit_status_code(status),
            };
        }
        match exit_code(docs) {
            // The exit-code document only appears once the mounts succeeded, so
            // its absence means the sandbox never started (ADR-0007).
            Some(code) => Outcome::Exited { exit_code: code },
            None => Outcome::SandboxError {
                detail: first_line(stderr),
            },
        }
    }
}

/// True when the deadline fired; the process group has been signalled and reaped.
fn wait_with_deadline(child: &mut Child, timeout: Duration) -> bool {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return false,
            Ok(None) => {}
            Err(_) => return false,
        }
        if started.elapsed() >= timeout {
            terminate(child);
            return true;
        }
        std::thread::sleep(POLL);
    }
}

/// Signs the whole group politely, then kills it, so no orphan keeps working.
fn terminate(child: &mut Child) {
    let group = child.id() as i32;
    signal_group(group, libc::SIGTERM);
    let deadline = Instant::now() + KILL_GRACE;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(POLL);
    }
    signal_group(group, libc::SIGKILL);
}

fn signal_group(group: i32, signal: i32) {
    // SAFETY: `kill` with a negative pid signals a process group; the return
    // value is ignored because the group may already be gone.
    unsafe {
        libc::kill(-group, signal);
    }
}

/// A pipe whose ends are close-on-exec, so a grandchild cannot inherit the
/// status channel and keep it open past the command.
fn pipe() -> io::Result<(RawFd, RawFd)> {
    let mut fds = [0 as RawFd; 2];
    // SAFETY: `pipe2` writes exactly two descriptors into the array it is given.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((fds[0], fds[1]))
}

/// Reads a pipe to EOF on its own thread, so a full pipe never blocks the wait.
fn drain<R: Read + Send + 'static>(mut source: R) -> std::thread::JoinHandle<io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        source.read_to_end(&mut buffer)?;
        Ok(buffer)
    })
}

/// Joins one reader, keeping whatever it read and what went wrong, if anything.
fn collect(
    handle: Option<std::thread::JoinHandle<io::Result<Vec<u8>>>>,
) -> (Vec<u8>, Option<String>) {
    match handle {
        None => (Vec::new(), None),
        Some(handle) => match handle.join() {
            Ok(Ok(bytes)) => (bytes, None),
            Ok(Err(error)) => (Vec::new(), Some(error.to_string())),
            Err(_) => (Vec::new(), Some("the reader thread panicked".to_string())),
        },
    }
}
