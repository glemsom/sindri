use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use sindri_core::bash::{self, SANDBOX_PATH, TEMP_VARIABLE};
use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement, SYSTEM_SSH_CONFIG_D, agent_ssh_config};
use sindri_core::run::Cancel;
use tempfile::TempDir;

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

    fn workspace(&self) -> PathBuf {
        self.directory("workspace")
    }

    fn temp(&self) -> PathBuf {
        self.directory("temp")
    }

    fn outside(&self) -> PathBuf {
        self.directory("outside")
    }
}

fn boundary(ground: &Ground, enforcement: Enforcement, timeout_seconds: u64) -> Boundary {
    let config = Config {
        timeout: Some(timeout_seconds),
        ..Config::default()
    };
    Boundary::from_config(&config, &ground.workspace(), enforcement).expect("the boundary holds")
}

fn said(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    match stderr.is_empty() {
        true => output.status.to_string(),
        false => stderr.to_owned(),
    }
}

fn sandbox() -> Option<()> {
    let problem = match Command::new(bash::BWRAP).arg("--version").output() {
        Err(error) => Some(format!("bwrap not on PATH: {error}")),
        Ok(version) if !version.status.success() => {
            Some(format!("bwrap --version failed: {}", said(&version)))
        }
        Ok(_) => match Command::new(bash::BWRAP)
            .args(["--unshare-all", "--ro-bind", "/", "/", "--dev", "/dev"])
            .args(["--proc", "/proc", "--", "true"])
            .output()
        {
            Err(error) => Some(format!("bwrap could not be run: {error}")),
            Ok(probe) if !probe.status.success() => Some(format!(
                "bwrap cannot create an unprivileged sandbox: {}",
                said(&probe)
            )),
            Ok(_) => None,
        },
    };

    let Some(reason) = problem else {
        return Some(());
    };
    let line = format!("sandbox tier: skipped (bwrap unavailable: {reason})\n");
    let _ = std::io::stderr().write_all(line.as_bytes());
    None
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.display())
}

#[test]
fn a_write_inside_the_writable_set_succeeds() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Bubblewrap, 120);
    let inside = ground.workspace().join("inside.txt");

    let outcome = bash::run(
        &boundary,
        &ground.temp(),
        &format!("echo written > {}", quoted(&inside)),
        &Cancel::new(),
    )
    .expect("the command runs");

    assert_eq!(outcome.code, Some(0), "stderr: {}", outcome.stderr);
    assert!(!outcome.timed_out);
    assert_eq!(
        std::fs::read_to_string(&inside).expect("the file the command wrote"),
        "written\n",
        "a path in the writable set is writable"
    );
}

#[test]
fn a_write_outside_the_writable_set_fails() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Bubblewrap, 120);
    let outside = ground.outside().join("escaped.txt");

    let outcome = bash::run(
        &boundary,
        &ground.temp(),
        &format!("echo escaped > {}", quoted(&outside)),
        &Cancel::new(),
    )
    .expect("the command runs");

    assert_ne!(
        outcome.code,
        Some(0),
        "a write outside the set must fail, not succeed quietly"
    );
    assert!(
        outcome.stderr.contains("Read-only file system"),
        "the kernel is what refuses it, not a check of ours: {}",
        outcome.stderr
    );
    assert!(
        !outside.exists(),
        "the boundary held: {} must not exist",
        outside.display()
    );
}

#[test]
fn the_host_root_is_read_only() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Bubblewrap, 120);
    let probe = format!("/sindri-bash-probe-{}", std::process::id());

    let outcome = bash::run(
        &boundary,
        &ground.temp(),
        &format!("echo probe > {probe}"),
        &Cancel::new(),
    )
    .expect("the command runs");

    assert_ne!(outcome.code, Some(0), "the host root is not writable");
    assert!(
        outcome.stderr.contains("Read-only file system"),
        "root is mounted read-only: {}",
        outcome.stderr
    );
    assert!(!Path::new(&probe).exists());
}

#[test]
fn the_session_temp_is_tmpdir_and_the_environment_is_only_that_and_a_path() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Bubblewrap, 120);
    let temp = ground.temp();

    let outcome = bash::run(
        &boundary,
        &temp,
        "printf '%s\\n%s\\n%s\\n' \"$TMPDIR\" \"$PATH\" \"${HOME-unset}\" \
         && echo scratch > \"$TMPDIR/scratch.txt\"",
        &Cancel::new(),
    )
    .expect("the command runs");
    let lines: Vec<&str> = outcome.stdout.lines().collect();

    assert_eq!(outcome.code, Some(0), "stderr: {}", outcome.stderr);
    assert_eq!(
        lines.first().copied(),
        Some(temp.display().to_string().as_str()),
        "the session temp is what TMPDIR names, so scratch is where it belongs"
    );
    assert_eq!(
        lines.get(1).copied(),
        Some(SANDBOX_PATH),
        "the sandbox's PATH is fixed, so two calls see one shell"
    );
    if std::env::var_os("HOME").is_some() {
        assert_eq!(
            lines.get(2).copied(),
            Some("unset"),
            "the sandbox gets no environment but TMPDIR and PATH, so a credential \
             in this process's environment cannot leak into a command"
        );
    }
    assert_eq!(
        std::fs::read_to_string(temp.join("scratch.txt")).expect("the scratch the command wrote"),
        "scratch\n",
        "the session temp is writable, so scratch is the one place besides the set"
    );
    assert_eq!(TEMP_VARIABLE, "TMPDIR");
}

#[test]
fn a_command_past_the_timeout_is_killed_and_says_timed_out() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Bubblewrap, 1);

    let started = Instant::now();
    let outcome =
        bash::run(&boundary, &ground.temp(), "sleep 30", &Cancel::new()).expect("the command runs");
    let elapsed = started.elapsed();

    assert!(outcome.timed_out, "the result says timed_out");
    assert_eq!(outcome.code, None, "a killed command has no exit code");
    assert!(
        elapsed < Duration::from_secs(10),
        "the call returned in {elapsed:?}, so the command was killed and not waited out"
    );
}

#[test]
fn an_unsafe_run_inherits_the_environment_and_the_cwd() {
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Unsafe, 120);
    let path = std::env::var("PATH").expect("this process has a PATH");
    let here = std::env::current_dir().expect("this process has a cwd");

    let outcome = bash::run(
        &boundary,
        &ground.temp(),
        "printf '%s\\n%s' \"$PATH\" \"$PWD\"",
        &Cancel::new(),
    )
    .expect("the command runs");
    let lines: Vec<&str> = outcome.stdout.lines().collect();

    assert_eq!(outcome.code, Some(0), "stderr: {}", outcome.stderr);
    assert_eq!(
        lines.first().copied(),
        Some(path.as_str()),
        "an unsafe run inherits the environment"
    );
    assert_eq!(
        lines.get(1).copied(),
        Some(here.display().to_string().as_str()),
        "an unsafe run inherits the cwd"
    );
}

#[test]
fn an_unsafe_run_says_unsafe_in_the_outcome_the_json_and_the_rail() {
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Unsafe, 120);

    let outcome =
        bash::run(&boundary, &ground.temp(), "true", &Cancel::new()).expect("the command runs");
    let json = serde_json::to_value(&outcome).expect("the outcome serialises");

    assert_eq!(outcome.enforcement.word(), "unsafe");
    assert_eq!(outcome.enforcement.rail_label(), "unsafe");
    assert_eq!(json["enforcement"], serde_json::Value::from("unsafe"));
    assert_eq!(json["timed_out"], serde_json::Value::from(false));
    assert_eq!(json["code"], serde_json::Value::from(0));
}

fn waited_for(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Waits until nothing is left of `pid`: a killed process stays listed in `/proc` until it is
/// reaped, and the parent that would reap this one was killed with it.
fn gone(pid: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

#[test]
fn a_cancel_kills_the_command_in_flight_and_the_child_it_started() {
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Unsafe, 120);
    let outside = ground.outside();
    let temp = ground.temp();
    let pidfile = outside.join("sleep.pid");
    let cancel = Cancel::new();
    let interrupted = cancel.clone();

    // A `sleep` left in the background is what proves the *group* was killed: a shell that `exec`s
    // one command is one pid, and killing one pid would pass a test that a `Ctrl-C` on a run of two
    // commands would not.
    let command = format!("sleep 30 & echo $! > {}; wait", quoted(&pidfile));
    let started = Instant::now();
    let calling = {
        let boundary = boundary.clone();
        let temp = temp.clone();
        std::thread::spawn(move || bash::run(&boundary, &temp, &command, &cancel))
    };

    assert!(
        waited_for(&pidfile),
        "the command started, so there is something in flight to cancel"
    );
    interrupted.now();
    let outcome = calling
        .join()
        .expect("the call returns")
        .expect("the command runs");
    let elapsed = started.elapsed();

    assert!(
        outcome.cancelled,
        "the outcome says the user's word stopped it, not a timeout: {outcome:?}"
    );
    assert_eq!(outcome.code, None, "a killed command has no exit code");
    assert!(
        elapsed < Duration::from_secs(10),
        "the call returned in {elapsed:?}, so the command was killed and not waited out"
    );

    let pid = std::fs::read_to_string(&pidfile)
        .expect("the pid the command wrote")
        .trim()
        .to_owned();
    assert!(
        gone(&pid),
        "the sleep the command started is gone, and not only the command: pid {pid}"
    );
}

#[test]
fn a_cancel_kills_a_sandboxed_command_before_it_can_write() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Bubblewrap, 120);
    let writable = ground.workspace();
    let temp = ground.temp();
    let late = writable.join("late.txt");
    let started_file = writable.join("started.txt");
    let cancel = Cancel::new();
    let interrupted = cancel.clone();

    let command = format!(
        "touch {}; sleep 2; echo late > {}",
        quoted(&started_file),
        quoted(&late)
    );
    let calling = {
        let boundary = boundary.clone();
        let temp = temp.clone();
        std::thread::spawn(move || bash::run(&boundary, &temp, &command, &cancel))
    };

    assert!(
        waited_for(&started_file),
        "the sandboxed command started, so there is something in flight to cancel"
    );
    interrupted.now();
    let outcome = calling
        .join()
        .expect("the call returns")
        .expect("the command runs");
    assert!(outcome.cancelled, "the user's word stopped it: {outcome:?}");

    std::thread::sleep(Duration::from_secs(3));
    assert!(
        !late.exists(),
        "nothing in the sandbox was left running to write {}",
        late.display()
    );
}

fn isolated_supervisor_regression(name: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("SINDRI_SUPERVISOR_REGRESSION", name)
        .process_group(0)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "isolated regression failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            child.wait().unwrap();
            panic!("supervisor setup hung");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn inherited_blocked_sigint_does_not_turn_signal_termination_into_success() {
    let name = "inherited_blocked_sigint_does_not_turn_signal_termination_into_success";
    if std::env::var("SINDRI_SUPERVISOR_REGRESSION").as_deref() != Ok(name) {
        isolated_supervisor_regression(name);
        return;
    }
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Unsafe, 2);
    unsafe {
        let mut mask = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        libc::sigaddset(&mut mask, libc::SIGINT);
        assert_eq!(
            libc::pthread_sigmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut()),
            0
        );
    }
    let outcome = bash::run(
        &boundary,
        &ground.temp(),
        "exec python3 -c 'import signal,os; signal.signal(signal.SIGINT, signal.SIG_DFL); signal.pthread_sigmask(signal.SIG_UNBLOCK, {signal.SIGINT}); os.kill(os.getpid(), signal.SIGINT)'",
        &Cancel::new(),
    ).unwrap();
    assert_eq!(outcome.code, None, "{outcome:?}");
    assert!(!outcome.timed_out);
}

#[test]
fn denied_close_range_refuses_setup_without_waiting_for_runner() {
    let name = "denied_close_range_refuses_setup_without_waiting_for_runner";
    if std::env::var("SINDRI_SUPERVISOR_REGRESSION").as_deref() != Ok(name) {
        isolated_supervisor_regression(name);
        return;
    }
    let ground = Ground::new();
    let boundary = boundary(&ground, Enforcement::Unsafe, 1);
    let temp = ground.temp();
    let mut filter = [
        libc::sock_filter {
            code: 0x20,
            jt: 0,
            jf: 0,
            k: 0,
        },
        libc::sock_filter {
            code: 0x15,
            jt: 0,
            jf: 1,
            k: libc::SYS_close_range as u32,
        },
        libc::sock_filter {
            code: 0x06,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ERRNO | libc::EPERM as u32,
        },
        libc::sock_filter {
            code: 0x06,
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ALLOW,
        },
    ];
    let program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };
    unsafe {
        assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
        assert_eq!(
            libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program),
            0
        );
    }
    let result = bash::run(&boundary, &temp, "sleep 30", &Cancel::new());
    assert!(
        matches!(result, Err(bash::BashError::Process { .. })),
        "{result:?}"
    );
}

#[test]
fn a_command_may_write_the_record_of_hosts_and_nothing_beside_it() {
    if sandbox().is_none() {
        return;
    }
    let ground = Ground::new();
    let ssh = ground.directory("ssh");
    let hosts = ssh.join("known_hosts");
    std::fs::write(&hosts, "192.168.50.21 ssh-ed25519 AAAA\n").expect("a record of hosts");
    let hosts = std::fs::canonicalize(&hosts).expect("the record resolves");
    let boundary =
        boundary(&ground, Enforcement::Bubblewrap, 120).with_known_hosts(Some(hosts.clone()));

    let outcome = bash::run(
        &boundary,
        &ground.temp(),
        &format!(
            "printf 'example.test ssh-ed25519 BBBB\n' >> {}; touch {}/probe || true",
            quoted(&hosts),
            quoted(&ssh)
        ),
        &Cancel::new(),
    )
    .expect("the command runs");

    assert_eq!(outcome.code, Some(0), "stderr: {}", outcome.stderr);
    assert!(
        std::fs::read_to_string(&hosts)
            .expect("the record of hosts")
            .contains("example.test"),
        "a record of hosts the launch granted takes the host a command met: {}",
        std::fs::read_to_string(&hosts).expect("the record of hosts")
    );
    assert!(
        !ssh.join("probe").exists(),
        "and the directory it sits in stays read-only"
    );
}

#[test]
fn a_command_reads_the_ssh_drop_ins_the_launch_staged_for_it() {
    if sandbox().is_none() || !Path::new(SYSTEM_SSH_CONFIG_D).is_dir() {
        return;
    }
    let ground = Ground::new();
    let host = ground.directory("host-drop-ins");
    std::fs::write(host.join("20-staged.conf"), "Host staged-marker\n").expect("a drop-in");
    let config = Config {
        ssh_agent: Some(true),
        ..Config::default()
    };
    let staged = agent_ssh_config(&config, &host, &ground.temp()).expect("the drop-ins are staged");
    let boundary = boundary(&ground, Enforcement::Bubblewrap, 120).with_ssh_config(Some(staged));

    let outcome = bash::run(
        &boundary,
        &ground.temp(),
        &format!("cat {SYSTEM_SSH_CONFIG_D}/20-staged.conf"),
        &Cancel::new(),
    )
    .expect("the command runs");

    assert_eq!(outcome.code, Some(0), "stderr: {}", outcome.stderr);
    assert!(
        outcome.stdout.contains("staged-marker"),
        "the drop-in directory ssh reads is the one the launch staged: {}",
        outcome.stdout
    );
}
