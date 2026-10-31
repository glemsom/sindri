//! The bash-executor port: the turn loop must be testable without bubblewrap,
//! so a scripted fake has to satisfy the same interface as the real executor.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

use std::sync::{Arc, Mutex};

use sindri_core::exec::{BashExecutor, Outcome, ToolResult, timeout_from_secs};

struct Scripted {
    replies: Mutex<Vec<ToolResult>>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Scripted {
    fn new(replies: Vec<ToolResult>, seen: Arc<Mutex<Vec<String>>>) -> Self {
        Scripted {
            replies: Mutex::new(replies),
            seen,
        }
    }
}

impl BashExecutor for Scripted {
    fn execute(&self, script: &str) -> Option<ToolResult> {
        self.seen.lock().expect("seen").push(script.to_string());
        Some(self.replies.lock().expect("replies").remove(0))
    }
}

fn name(result: Option<ToolResult>) -> String {
    result.expect("the run was not cancelled").stdout
}

fn exited(stdout: &str) -> ToolResult {
    ToolResult {
        outcome: Outcome::Exited { exit_code: 0 },
        stdout: stdout.to_string(),
        stderr: String::new(),
        truncated: false,
        spill: Vec::new(),
        duration_ms: 1,
    }
}

#[test]
fn a_scripted_executor_satisfies_the_port_without_a_sandbox() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let executor: Box<dyn BashExecutor> = Box::new(Scripted::new(
        vec![exited("first"), exited("second")],
        Arc::clone(&seen),
    ));

    assert_eq!(name(executor.execute("echo one")), "first");
    assert_eq!(name(executor.execute("echo two")), "second");
    assert_eq!(*seen.lock().expect("seen"), vec!["echo one", "echo two"]);
}

#[test]
fn the_port_crosses_threads_so_the_loop_can_hold_it() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let executor: Arc<dyn BashExecutor> =
        Arc::new(Scripted::new(vec![exited("from another thread")], seen));

    let worker = {
        let executor = Arc::clone(&executor);
        std::thread::spawn(move || name(executor.execute("true")))
    };
    assert_eq!(worker.join().expect("thread"), "from another thread");
}

#[test]
fn the_deadline_is_bounded_at_both_ends() {
    assert_eq!(
        timeout_from_secs(120).expect("default"),
        std::time::Duration::from_secs(120)
    );
    assert_eq!(
        timeout_from_secs(3600).expect("the cap"),
        std::time::Duration::from_secs(3600)
    );
    assert!(
        timeout_from_secs(0).is_err(),
        "no deadline is not a deadline"
    );
    assert!(timeout_from_secs(3601).is_err(), "the cap is 3600 seconds");
}

#[test]
fn the_real_executor_honours_the_unsafe_enforcement() {
    use sindri_core::sandbox::Enforcement;

    let scratch = tempfile::TempDir::new().expect("scratch");
    let session_temp = scratch.path().join("tmp");
    std::fs::create_dir_all(session_temp.join("home")).expect("session temp");
    let workspace = scratch.path().canonicalize().expect("canonical");
    let policy = sindri_core::session::policy_for(
        &workspace,
        &[],
        session_temp,
        sindri_core::session::search_path(),
        Enforcement::YoloUnsafe,
    )
    .expect("policy");

    let executor = sindri_core::exec::SandboxExecutor::new(
        policy,
        std::time::Duration::from_secs(120),
        scratch.path().join("spill"),
        sindri_core::cancel::Cancel::new(),
    );
    let result = executor
        .execute("echo unsafe-but-running; exit 7")
        .expect("the run was not cancelled");

    assert_eq!(result.outcome, Outcome::Exited { exit_code: 7 });
    assert!(
        result.stdout.contains("unsafe-but-running"),
        "{}",
        result.stdout
    );
}

#[test]
fn a_cancelled_executor_kills_its_command_group_and_reports_no_result() {
    use sindri_core::cancel::Cancel;
    use sindri_core::sandbox::Enforcement;

    let scratch = tempfile::TempDir::new().expect("scratch");
    let session_temp = scratch.path().join("tmp");
    std::fs::create_dir_all(session_temp.join("home")).expect("session temp");
    let workspace = scratch.path().canonicalize().expect("canonical");
    let policy = sindri_core::session::policy_for(
        &workspace,
        &[],
        session_temp,
        sindri_core::session::search_path(),
        Enforcement::Bubblewrap,
    )
    .expect("policy");

    let cancel = Cancel::new();
    let executor = sindri_core::exec::SandboxExecutor::new(
        policy,
        std::time::Duration::from_secs(120),
        scratch.path().join("spill"),
        cancel.clone(),
    );

    // The command says where it got to, then waits long enough that only a
    // killed process group can stop it.
    let started = workspace.join("started");
    let late = workspace.join("late");
    let script = format!(
        "touch {}; sleep 3; touch {}",
        started.display(),
        late.display()
    );
    let cancelling = {
        let started = started.clone();
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while !started.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the command never started"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            cancel.trigger();
        })
    };

    let result = executor.execute(&script);
    cancelling.join().expect("the cancelling thread");

    assert!(
        result.is_none(),
        "a cancelled command has no result to report"
    );
    // Long enough that the command would have written the marker had it lived.
    std::thread::sleep(std::time::Duration::from_secs(5));
    assert!(
        !late.exists(),
        "the cancelled command's group was killed, grandchildren included"
    );
}

/// One executor, the enforcement chosen, and the script it runs: the seam every
/// test below reads its result from.
fn executor_for(
    enforcement: sindri_core::sandbox::Enforcement,
) -> (tempfile::TempDir, sindri_core::exec::SandboxExecutor) {
    let scratch = tempfile::TempDir::new().expect("scratch");
    let session_temp = scratch.path().join("tmp");
    std::fs::create_dir_all(session_temp.join("home")).expect("session temp");
    let workspace = scratch.path().canonicalize().expect("canonical");
    let policy = sindri_core::session::policy_for(
        &workspace,
        &[],
        session_temp,
        sindri_core::session::search_path(),
        enforcement,
    )
    .expect("policy");

    let executor = sindri_core::exec::SandboxExecutor::new(
        policy,
        std::time::Duration::from_secs(120),
        scratch.path().join("spill"),
        sindri_core::cancel::Cancel::new(),
    );
    (scratch, executor)
}

const AMBIENT: &str = "printf '%s\\n%s\\n' \"$HOME\" \"$PWD\"";

#[test]
fn an_unsafe_child_inherits_the_ambient_home_and_working_directory() {
    let (_scratch, executor) = executor_for(sindri_core::sandbox::Enforcement::YoloUnsafe);
    let result = executor
        .execute(AMBIENT)
        .expect("the run was not cancelled");

    let mut lines = result.stdout.lines();
    assert_eq!(
        lines.next(),
        std::env::var("HOME").ok().as_deref(),
        "the whole ambient environment is inherited: {}",
        result.stdout
    );
    assert_eq!(
        lines.next(),
        std::env::current_dir()
            .ok()
            .and_then(|dir| dir.canonicalize().ok())
            .map(|dir| dir.display().to_string())
            .as_deref(),
        "and so is the caller's working directory: {}",
        result.stdout
    );
}

#[test]
fn a_sandboxed_child_still_sees_the_session_home_and_not_the_hosts() {
    let (_scratch, executor) = executor_for(sindri_core::sandbox::Enforcement::Bubblewrap);
    let result = executor
        .execute(AMBIENT)
        .expect("the run was not cancelled");

    let home = result.stdout.lines().next().unwrap_or_default();
    assert_ne!(
        Some(home),
        std::env::var("HOME").ok().as_deref(),
        "the sandbox remaps HOME to its own: {}",
        result.stdout
    );
    assert!(
        home.ends_with("/tmp/home"),
        "HOME is the session temp's home: {}",
        result.stdout
    );
}
