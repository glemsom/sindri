//! The real executor: bwrap exactly as the policy derives it, our own deadline,
//! and an outcome classified from bwrap's status channel rather than guessed at
//! (ADR-0007).

use std::fs::File;
use std::io::{self, BufRead, Read};
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::result::{Outcome, OutputLimit, ToolResult, truncate};
use super::status::{exit_code, exit_status_code, first_line};
use super::tap::{self, OutputSink, Side};
use super::{BashExecutor, StatusSink};
use crate::cancel::Cancel;
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

/// Why a command stopped, before it becomes a result the model reads.
enum Done {
    /// The command finished; this is what the model is told.
    Result(ToolResult),
    /// The run was cancelled: the group is dead and there is nothing to report.
    Cancelled,
    /// The sandbox itself could not run the command.
    Broken(String),
}

/// Runs `bash` scripts inside the sandbox described by one policy.
pub struct SandboxExecutor {
    policy: SandboxPolicy,
    timeout: Duration,
    spill_dir: PathBuf,
    cancel: Cancel,
    limit: OutputLimit,
    /// Where bubblewrap's own account of each command is handed over as it
    /// arrives, so a run that never finished still leaves the record.
    status: Option<Arc<dyn StatusSink>>,
    /// Where each command's output is handed over as it arrives, for a front
    /// end showing the tail of a running command.
    output: Option<Arc<dyn OutputSink>>,
    /// Names each command's spill files, so turn n's path is still turn n's.
    calls: AtomicU64,
}

impl SandboxExecutor {
    /// Builds an executor for one session: its policy, its deadline, where
    /// over-long output is spilled, and the flag that stops it mid-command.
    pub fn new(
        policy: SandboxPolicy,
        timeout: Duration,
        spill_dir: PathBuf,
        cancel: Cancel,
    ) -> Self {
        SandboxExecutor {
            policy,
            timeout,
            spill_dir,
            cancel,
            limit: OutputLimit::default(),
            status: None,
            output: None,
            calls: AtomicU64::new(0),
        }
    }

    /// Records bubblewrap's status documents as they arrive (ADR-0006).
    #[must_use]
    pub fn with_status_sink(mut self, status: Arc<dyn StatusSink>) -> Self {
        self.status = Some(status);
        self
    }

    /// Hands each line of a running command's output over as it arrives, so a
    /// human can tell a slow build from a hung one (ADR-0011 §9).
    #[must_use]
    pub fn with_output_sink(mut self, output: Arc<dyn OutputSink>) -> Self {
        self.output = Some(output);
        self
    }
}

impl BashExecutor for SandboxExecutor {
    fn execute(&self, script: &str) -> Option<ToolResult> {
        let started = Instant::now();
        match self.run(script, started) {
            Done::Result(result) => Some(result),
            Done::Cancelled => None,
            Done::Broken(detail) => Some(ToolResult {
                outcome: Outcome::SandboxError { detail },
                stdout: String::new(),
                stderr: String::new(),
                truncated: false,
                spill: Vec::new(),
                duration_ms: started.elapsed().as_millis() as u64,
            }),
        }
    }
}

impl SandboxExecutor {
    fn run(&self, script: &str, started: Instant) -> Done {
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

        // Numbered before the spawn, because the status thread has to name the
        // command it is describing while it is still running.
        let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        let mut status_pipe = None;
        if sandboxed {
            let (read_end, write_end) = match pipe() {
                Ok(ends) => ends,
                Err(error) => return Done::Broken(error.to_string()),
            };
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

        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(error) => return Done::Broken(error.to_string()),
        };

        // Release our own write end so the reader can reach EOF, then drain all
        // three channels while the child runs.
        let status_docs = status_pipe.take().map(|(writer, reader)| {
            drop(writer);
            drain_documents(reader, call, self.status.clone())
        });
        let stdout = child
            .stdout
            .take()
            .map(|source| tap::drain(source, Side::Stdout, self.output.clone()));
        let stderr = child
            .stderr
            .take()
            .map(|source| tap::drain(source, Side::Stderr, self.output.clone()));

        let stopped = wait_until_done(&mut child, self.timeout, &self.cancel);
        let status = match child.wait() {
            Ok(status) => status,
            Err(error) => return Done::Broken(error.to_string()),
        };
        if stopped == Stop::Cancelled {
            return Done::Cancelled;
        }
        let timed_out = stopped == Stop::TimedOut;

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

        Done::Result(ToolResult {
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

/// Why the wait ended: the command finished, our deadline fired, or the run was
/// cancelled. In the last two cases the process group has been signalled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Finished,
    TimedOut,
    Cancelled,
}

/// Waits for the command, killing its whole group the moment the deadline fires
/// or the run is cancelled, so nothing outlives the reason it was stopped.
fn wait_until_done(child: &mut Child, timeout: Duration, cancel: &Cancel) -> Stop {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Stop::Finished,
            Ok(None) => {}
            Err(_) => return Stop::Finished,
        }
        if cancel.is_triggered() {
            terminate(child);
            return Stop::Cancelled;
        }
        if started.elapsed() >= timeout {
            terminate(child);
            return Stop::TimedOut;
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

/// Reads the status channel one document at a time, handing each to the sink the
/// moment it is complete and keeping the lot for classification: a run killed
/// between two documents has still recorded the ones that arrived.
fn drain_documents<R: Read + Send + 'static>(
    source: R,
    call: u64,
    sink: Option<Arc<dyn StatusSink>>,
) -> std::thread::JoinHandle<io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut reader = io::BufReader::new(source);
        let mut seen = Vec::new();
        loop {
            let mut line = Vec::new();
            if reader.read_until(b'\n', &mut line)? == 0 {
                return Ok(seen);
            }
            seen.extend_from_slice(&line);
            if let Some(sink) = &sink {
                let document = String::from_utf8_lossy(&line);
                let document = document.trim();
                if !document.is_empty() {
                    sink.document(call, document);
                }
            }
        }
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
