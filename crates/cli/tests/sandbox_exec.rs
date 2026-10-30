//! The process seam for the sandbox debug subcommand: every acceptance
//! criterion of the executor, proven without a model in the loop.
//! Fields are asserted one by one rather than snapshotted: the batch contract's
//! JSON shape (ADR-0004) is fixed in T6 with `insta`, and this subcommand's
//! shape is not that contract.

mod common;

use common::Env;
use serde_json::Value;

fn exec(env: &Env, command: &[&str]) -> Value {
    exec_with(env, &[], command)
}

fn exec_with(env: &Env, flags: &[&str], command: &[&str]) -> Value {
    let assert = env
        .cmd()
        .args(["sandbox", "exec"])
        .args(flags)
        .arg("--")
        .args(command)
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("not JSON: {error}\n{stdout}"))
}

#[test]
fn a_command_runs_inside_the_sandbox_and_reports_its_streams() {
    let env = Env::new();
    let out = exec(&env, &["bash", "-c", "echo out; echo err >&2; exit 0"]);

    assert_eq!(out["outcome"], "exited");
    assert_eq!(out["exit_code"], 0);
    assert_eq!(out["stdout"], "out\n");
    assert_eq!(out["stderr"], "err\n");
    assert_eq!(out["truncated"], false);
    assert!(out["duration_ms"].is_u64());
}

#[test]
fn the_command_cannot_see_processes_outside_its_namespace() {
    let env = Env::new();
    let mut outsider = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn an outside process");
    let outsider_pid = outsider.id();

    let script = format!("test -e /proc/{outsider_pid} && echo visible || echo hidden");
    let out = exec(&env, &["bash", "-c", &script]);
    let _ = outsider.kill();
    let _ = outsider.wait();

    assert_eq!(out["outcome"], "exited");
    assert_eq!(
        out["stdout"].as_str().expect("stdout").trim(),
        "hidden",
        "pid {outsider_pid} must not exist inside the sandbox"
    );
}

#[test]
fn a_write_outside_the_writable_set_is_refused_by_the_kernel() {
    let env = Env::new();
    let out = exec(&env, &["bash", "-c", "echo probe > /etc/sindri-probe"]);

    assert_eq!(out["outcome"], "exited");
    assert_ne!(out["exit_code"], 0, "the write must fail: {out}");
    let stderr = out["stderr"].as_str().expect("stderr");
    assert!(
        stderr.contains("Read-only file system"),
        "the kernel's own words survive: {stderr}"
    );
    let escaped = std::path::Path::new("/etc/sindri-probe");
    assert!(!escaped.exists(), "nothing escaped the sandbox");
}

#[test]
fn the_script_status_is_the_status_of_its_last_command_and_pipefail_is_available() {
    let env = Env::new();
    assert_eq!(exec(&env, &["bash", "-c", "false | true"])["exit_code"], 0);
    assert_eq!(
        exec(&env, &["bash", "-c", "set -o pipefail; false | true"])["exit_code"],
        1,
        "pipefail reports the failing stage"
    );
    assert_eq!(exec(&env, &["bash", "-c", "exit 3"])["exit_code"], 3);
}

#[test]
fn a_pipeline_hides_a_failing_stage_so_pipestatus_is_available() {
    let env = Env::new();
    // The refusal belongs to the first stage; the pipeline still exits 0, which
    // is exactly why a bare exit code is not an answer (ADR-0007).
    let out = exec(
        &env,
        &[
            "bash",
            "-c",
            "echo probe > /etc/sindri-probe | head -1; echo \"stage=${PIPESTATUS[0]}\"",
        ],
    );

    assert_eq!(out["outcome"], "exited");
    assert_eq!(out["exit_code"], 0, "the last stage decides: {out}");
    let stdout = out["stdout"].as_str().expect("stdout");
    assert!(
        stdout.contains("stage=1"),
        "the failing stage is visible through PIPESTATUS: {stdout:?}"
    );
}

#[test]
fn the_default_deadline_is_generous_but_present() {
    let env = Env::new();
    // No --timeout: the default must be the contract's 120 seconds, so a short
    // command finishes normally rather than being cut at some smaller number.
    let out = exec(&env, &["bash", "-c", "sleep 2; echo done"]);
    assert_eq!(out["outcome"], "exited", "{out}");
    assert!(
        out["duration_ms"].as_u64().expect("duration") >= 2000,
        "{out}"
    );
}

#[test]
fn a_missing_binary_is_a_command_failure_not_a_sandbox_failure() {
    let env = Env::new();
    let out = exec(&env, &["bash", "-c", "sindri-no-such-binary"]);

    assert_eq!(out["outcome"], "exited", "{out}");
    assert_eq!(out["exit_code"], 127);
    let stderr = out["stderr"].as_str().expect("stderr");
    assert!(stderr.contains("sindri-no-such-binary"), "{stderr}");
}

#[test]
fn a_command_that_outlives_its_deadline_times_out_without_an_exit_code() {
    let env = Env::new();
    let started = std::time::Instant::now();
    let out = exec_with(&env, &["--timeout", "1"], &["sleep", "30"]);

    assert_eq!(out["outcome"], "timed_out", "{out}");
    assert!(
        out.get("exit_code").is_none(),
        "a timeout is our own signal, never an exit code: {out}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "the deadline fired instead of waiting for the command"
    );
}

#[test]
fn a_timed_out_command_keeps_the_output_it_produced_first() {
    let env = Env::new();
    let out = exec_with(
        &env,
        &["--timeout", "1"],
        &["bash", "-c", "echo started; sleep 30"],
    );

    assert_eq!(out["outcome"], "timed_out", "{out}");
    let stdout = out["stdout"].as_str().expect("stdout");
    assert!(
        stdout.contains("started"),
        "partial output survives: {stdout:?}"
    );
}

#[test]
fn a_timed_out_command_takes_its_whole_process_group_with_it() {
    let env = Env::new();
    let marker = env.workspace.join("late-writer");
    let script = format!("sleep 3; touch {}", marker.display());
    exec_with(&env, &["--timeout", "1"], &["bash", "-c", &script]);

    std::thread::sleep(std::time::Duration::from_secs(4));
    assert!(
        !marker.exists(),
        "a grandchild must not outlive the deadline"
    );
}

#[test]
fn oversized_output_is_truncated_head_and_tail_and_spilled_in_full() {
    let env = Env::new();
    let out = exec(&env, &["bash", "-c", "seq 1 200000"]);

    assert_eq!(out["outcome"], "exited", "{out}");
    assert_eq!(out["truncated"], true, "{out}");
    let stdout = out["stdout"].as_str().expect("stdout");
    assert!(
        stdout.contains("1\n2\n3\n"),
        "the head is kept: {stdout:.80}"
    );
    assert!(stdout.contains("200000"), "the tail is kept");
    assert!(stdout.contains("bytes omitted"), "{stdout:.200}");

    let spill = out["spill"][0].as_str().expect("spill path");
    let full = std::fs::read_to_string(spill).expect("spill file exists");
    assert!(
        full.ends_with("200000\n"),
        "the spill file holds everything"
    );
    assert!(
        full.len() > 1_000_000,
        "no data was lost: {} bytes",
        full.len()
    );
    assert!(stdout.contains(spill), "the model is told where to look");
}

#[test]
fn a_sandbox_that_never_starts_is_a_sandbox_error_not_a_failed_command() {
    let env = Env::new();
    let path = common::path_without(
        &common::required_binaries(),
        &["bwrap"],
        &common::host_path(),
    );
    let fake = path.path().join("bwrap");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho 'setting up uid map: Permission denied' >&2\nexit 1\n",
    )
    .expect("write fake bwrap");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let output = env
        .cmd()
        .args(["sandbox", "exec", "--", "bash", "-c", "echo never"])
        .env("PATH", path.path())
        .assert()
        .code(3)
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let value: Value = serde_json::from_str(&stdout).expect("JSON outcome");

    assert_eq!(value["outcome"], "sandbox_error", "{value}");
    assert!(value.get("exit_code").is_none(), "no exit code: {value}");
    let detail = value["detail"].as_str().expect("detail");
    assert!(detail.contains("Permission denied"), "{detail}");
}

#[test]
fn a_deadline_beyond_the_cap_is_a_usage_error() {
    let env = Env::new();
    env.cmd()
        .args(["sandbox", "exec", "--timeout", "5000", "--", "echo", "hi"])
        .assert()
        .code(2)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("3600"));
}

#[test]
fn each_command_spills_to_its_own_file_so_an_earlier_path_keeps_its_text() {
    let env = Env::new();
    let first = exec(&env, &["bash", "-c", "seq 1 200000"]);
    let first_spill = first["spill"][0].as_str().expect("first spill").to_string();
    let second = exec(&env, &["bash", "-c", "seq 1 200000 | tr 1 9"]);
    let second_spill = second["spill"][0]
        .as_str()
        .expect("second spill")
        .to_string();

    assert_ne!(
        first_spill, second_spill,
        "a later call must not reuse the path"
    );
    let text = std::fs::read_to_string(&first_spill).expect("the first spill still exists");
    assert!(
        text.ends_with("200000\n") && text.starts_with("1\n2\n"),
        "the first call's full output is still there"
    );
}
