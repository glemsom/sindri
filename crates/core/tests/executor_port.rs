#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken contract reports itself"
)]

//! The bash-executor port: the turn loop must be testable without bubblewrap,
//! so a scripted fake has to satisfy the same interface as the real executor.

use std::sync::{Arc, Mutex};

use sindri_core::exec::{BashExecutor, Outcome, ToolResult, timeout_from_secs};

/// Stands in for the sandbox: answers from a script and records what it saw.
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
    fn execute(&self, script: &str) -> ToolResult {
        self.seen.lock().expect("seen").push(script.to_string());
        self.replies.lock().expect("replies").remove(0)
    }
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

    assert_eq!(executor.execute("echo one").stdout, "first");
    assert_eq!(executor.execute("echo two").stdout, "second");
    assert_eq!(*seen.lock().expect("seen"), vec!["echo one", "echo two"]);
}

#[test]
fn the_port_crosses_threads_so_the_loop_can_hold_it() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let executor: Arc<dyn BashExecutor> =
        Arc::new(Scripted::new(vec![exited("from another thread")], seen));

    let worker = {
        let executor = Arc::clone(&executor);
        std::thread::spawn(move || executor.execute("true").stdout)
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
    );
    let result = executor.execute("echo unsafe-but-running; exit 7");

    assert_eq!(result.outcome, Outcome::Exited { exit_code: 7 });
    assert!(
        result.stdout.contains("unsafe-but-running"),
        "{}",
        result.stdout
    );
}
