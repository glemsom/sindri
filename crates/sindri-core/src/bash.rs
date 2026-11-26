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

pub const SHELL: &str = "/bin/bash";

pub const SANDBOX_PATH: &str = "/usr/local/bin:/usr/bin:/bin";

pub const BWRAP: &str = "bwrap";

pub const TEMP_VARIABLE: &str = "TMPDIR";

const MAX_NAP: Duration = Duration::from_millis(25);

const SIGKILL: i32 = 9;
const SIGTERM: i32 = 15;
const PR_SET_PDEATHSIG: i32 = 1;
const PR_SET_CHILD_SUBREAPER: i32 = 36;

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
    run_with_handoff(boundary, session_temp, command, cancel, None)
}

pub fn run_with_handoff(
    boundary: &Boundary,
    session_temp: &Path,
    command: &str,
    cancel: &Cancel,
    descriptor: Option<i32>,
) -> Result<Outcome, BashError> {
    use std::os::fd::AsRawFd;
    let inherited = descriptor
        .map(|fd| std::fs::File::open(format!("/proc/self/fd/{fd}")))
        .transpose()
        .map_err(|error| BashError::Process {
            program: SHELL.to_owned(),
            reason: error.to_string(),
        })?;
    let descriptor = inherited.as_ref().map(AsRawFd::as_raw_fd);
    let mut invocation = invocation(boundary, session_temp, command)?;
    if descriptor.is_some() && boundary.enforcement() != Enforcement::Unsafe {
        let index = invocation.args.len() - 4;
        invocation.args.splice(
            index..index,
            [
                "--ro-bind-data".to_owned(),
                "198".to_owned(),
                session_temp.join("launch.json").display().to_string(),
            ],
        );
    }
    let program = invocation.program().to_owned();

    // No stdin: a command that reads one would hang the run. It leads a process group of its
    // own, so one kill takes everything it started.
    let parent = std::process::id();
    let mut command = Command::new(&program);
    unsafe {
        command.pre_exec(move || {
            if prctl(PR_SET_PDEATHSIG, SIGKILL, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if getppid() != parent as i32 {
                return Err(std::io::Error::from_raw_os_error(libc::ECHILD));
            }
            if prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut blocked: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut blocked);
            libc::sigaddset(&mut blocked, SIGTERM);
            let mut previous: libc::sigset_t = std::mem::zeroed();
            if libc::sigprocmask(libc::SIG_BLOCK, &blocked, &mut previous) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let fd = libc::open(c"/proc/thread-self/children".as_ptr(), libc::O_RDONLY);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let supervisor = getpid();
            let root = fork();
            if root < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if root > 0 {
                // The supervisor must release Command's exec-error pipe before waiting.
                if dup2(fd, 199) < 0 {
                    _exit(127);
                }
                if close_range(3, 198, 0) != 0 || close_range(200, u32::MAX, 0) != 0 {
                    let error = std::io::Error::last_os_error();
                    kill(root, SIGKILL);
                    while waitpid(root, std::ptr::null_mut(), 0) < 0
                        && *libc::__errno_location() == libc::EINTR
                    {}
                    return Err(error);
                }
                let mut status = 0;
                loop {
                    let waited = waitpid(root, &mut status, libc::WNOHANG);
                    if waited == root {
                        break;
                    }
                    if waited < 0 && *libc::__errno_location() != libc::EINTR {
                        _exit(127);
                    }
                    let poll = libc::timespec {
                        tv_sec: 0,
                        tv_nsec: 25_000_000,
                    };
                    if libc::sigtimedwait(&blocked, std::ptr::null_mut(), &poll) == SIGTERM {
                        status = SIGTERM;
                        break;
                    }
                }
                // Adoption after each reap exposes the next generation, regardless of thread ownership.
                supervise_cleanup(199);
                if status & 127 != 0 {
                    let terminating = status & 127;
                    signal(terminating, 0);
                    libc::sigemptyset(&mut blocked);
                    libc::sigaddset(&mut blocked, terminating);
                    libc::sigprocmask(libc::SIG_UNBLOCK, &blocked, std::ptr::null_mut());
                    kill(getpid(), terminating);
                    _exit(128 + terminating);
                }
                _exit((status >> 8) & 255);
            }
            libc::close(fd);
            libc::sigprocmask(libc::SIG_SETMASK, &previous, std::ptr::null_mut());
            if prctl(PR_SET_PDEATHSIG, SIGKILL, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if getppid() != supervisor {
                return Err(std::io::Error::from_raw_os_error(libc::ECHILD));
            }
            if let Some(fd) = descriptor
                && (dup2(fd, 198) < 0 || fcntl(198, 2, 0) < 0)
            {
                return Err(std::io::Error::last_os_error());
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

    while !stdout.finished() || !stderr.finished() {
        if cancel.is_set() {
            cancelled = true;
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    Ok(Outcome {
        enforcement: boundary.enforcement(),
        code: status.and_then(|status| status.code()),
        timed_out,
        cancelled,
        stdout: String::from_utf8_lossy(&stdout.collected()).into_owned(),
        stderr: String::from_utf8_lossy(&stderr.collected()).into_owned(),
    })
}

fn stop(child: &mut Child) {
    unsafe {
        kill(child.id() as i32, SIGTERM);
    }
}

// After fork only stack storage and async-signal-safe syscalls may be used.
unsafe fn supervise_cleanup(fd: i32) {
    unsafe {
        loop {
            if libc::lseek(fd, 0, libc::SEEK_SET) < 0 {
                _exit(127);
            }
            let mut bytes = [0u8; 4096];
            let mut pid = 0i32;
            loop {
                let count = libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len());
                if count < 0 {
                    if *libc::__errno_location() == libc::EINTR {
                        continue;
                    }
                    _exit(127);
                }
                if count == 0 {
                    if pid > 0 {
                        kill(pid, SIGKILL);
                    }
                    break;
                }
                for &byte in &bytes[..count as usize] {
                    if byte.is_ascii_digit() {
                        pid = pid
                            .saturating_mul(10)
                            .saturating_add(i32::from(byte - b'0'));
                    } else if pid > 0 {
                        kill(pid, SIGKILL);
                        pid = 0;
                    }
                }
            }
            let mut status = 0;
            if waitpid(-1, &mut status, 0) < 0 {
                match *libc::__errno_location() {
                    libc::ECHILD => return,
                    libc::EINTR => continue,
                    _ => _exit(127),
                }
            }
        }
    }
}

// musl does not provide the close_range wrapper used by the supervisor.
unsafe fn close_range(first: u32, last: u32, flags: u32) -> i32 {
    unsafe { libc::syscall(libc::SYS_close_range, first, last, flags) as i32 }
}

unsafe extern "C" {
    fn dup2(old: i32, new: i32) -> i32;
    fn fcntl(fd: i32, command: i32, value: i32) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, arg2: i32, arg3: i32, arg4: i32, arg5: i32) -> i32;
    fn getppid() -> i32;
    fn getpid() -> i32;
    fn fork() -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn _exit(status: i32) -> !;
    fn signal(signum: i32, handler: usize) -> usize;
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::config::Config;

    fn exercise(command: &str, timeout: u64, cancel: Cancel) -> (Outcome, tempfile::TempDir) {
        let temp = tempfile::tempdir().unwrap();
        let config = Config {
            timeout: Some(timeout),
            ..Config::default()
        };
        let boundary = Boundary::from_config(&config, temp.path(), Enforcement::Unsafe).unwrap();
        let command = command.replace(
            "MARKER",
            &temp.path().join("survivor").display().to_string(),
        );
        let outcome = run(&boundary, temp.path(), &command, &cancel).unwrap();
        (outcome, temp)
    }

    #[test]
    fn early_exit_reaps_children_owned_by_non_leader_threads() {
        let (outcome, _) = exercise(
            r#"python3 -c '
import os, threading, time
ready = threading.Event()
def worker():
    pid = os.fork()
    if pid == 0:
        os.setsid()
        time.sleep(30)
        os._exit(0)
    print(pid, flush=True)
    ready.set()
    time.sleep(30)
threading.Thread(target=worker).start()
ready.wait()
os._exit(7)
'"#,
            5,
            Cancel::default(),
        );
        assert_eq!(outcome.code, Some(7));
        for pid in outcome.stdout.lines() {
            let path = format!("/proc/{pid}");
            let survived = Path::new(&path).exists();
            if survived {
                unsafe {
                    kill(pid.parse().unwrap(), SIGKILL);
                }
            }
            assert!(!survived, "thread-owned child {pid} survived the call");
        }
    }

    #[test]
    fn cancellation_reaps_concurrent_thread_forks() {
        let temp = tempfile::tempdir().unwrap();
        let config = Config {
            timeout: Some(5),
            ..Config::default()
        };
        let boundary = Boundary::from_config(&config, temp.path(), Enforcement::Unsafe).unwrap();
        let cancel = Cancel::default();
        let trigger = cancel.clone();
        let ready = temp.path().join("ready");
        let observed = ready.clone();
        let monitor = thread::spawn(move || {
            while !observed.exists() {
                thread::yield_now();
            }
            trigger.now();
        });
        let command = format!(
            r#"python3 -c '
import os, threading, time
lock = threading.Lock()
count = 0
def worker():
    global count
    while True:
        with lock:
            pid = os.fork()
            if pid == 0:
                os.setsid()
                time.sleep(30)
                os._exit(0)
            print(pid, flush=True)
            count += 1
            if count == 16:
                open("{}", "w").close()
        time.sleep(0.001)
for _ in range(4):
    threading.Thread(target=worker).start()
time.sleep(30)
'"#,
            ready.display()
        );
        let outcome = run(&boundary, temp.path(), &command, &cancel).unwrap();
        monitor.join().unwrap();
        assert!(outcome.cancelled);
        assert!(outcome.stdout.lines().count() >= 16);
        for pid in outcome.stdout.lines() {
            let survived = Path::new(&format!("/proc/{pid}")).exists();
            if survived {
                unsafe {
                    kill(pid.parse().unwrap(), SIGKILL);
                }
            }
            assert!(
                !survived,
                "concurrently forked child {pid} survived cancellation"
            );
        }
    }

    #[test]
    fn root_early_exit_stops_detached_descendants() {
        let (outcome, temp) = exercise(
            "setsid sh -c 'sleep 1; echo alive > MARKER' >/dev/null 2>&1 & sleep 0.1; exit 7",
            5,
            Cancel::default(),
        );
        assert_eq!(outcome.code, Some(7));
        thread::sleep(Duration::from_millis(1200));
        assert!(!temp.path().join("survivor").exists());
    }
    #[test]
    fn pipe_retention_remains_under_timeout() {
        let (outcome, temp) = exercise(
            "setsid sh -c 'sleep 2; echo alive > MARKER' & wait",
            1,
            Cancel::default(),
        );
        assert!(outcome.timed_out);
        thread::sleep(Duration::from_millis(1500));
        assert!(!temp.path().join("survivor").exists());
    }

    #[test]
    fn cancellation_stops_forking_detached_descendants() {
        let cancel = Cancel::default();
        let trigger = cancel.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            trigger.now();
        });
        let (outcome, temp) = exercise(
            "while :; do setsid sh -c 'sleep 1; echo alive >> MARKER' & sleep 0.01; done",
            5,
            cancel,
        );
        assert!(outcome.cancelled);
        thread::sleep(Duration::from_millis(1200));
        assert!(!temp.path().join("survivor").exists());
    }
}
