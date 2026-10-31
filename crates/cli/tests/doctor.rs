//! The process seam for `sindri doctor`: what a user sees before any model is
//! in the loop.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::{Env, path_without, required_binaries};
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn doctor_prints_the_sandbox_description_and_exits_zero() {
    let env = Env::new();
    let assert = env.cmd().arg("doctor").assert().success();

    let output = assert.get_output();
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "",
        "doctor is silent on success"
    );

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        stdout.ends_with('\n'),
        "the description ends with a newline"
    );
    assert!(
        !stdout.ends_with("\n\n"),
        "exactly one trailing newline: {stdout:?}"
    );

    let workspace = env.canonical_workspace();
    let workspace = workspace.to_str().expect("utf-8 workspace");
    assert!(
        stdout.contains(workspace),
        "the workspace is in the writable set: {stdout}"
    );
    assert!(stdout.contains("writable set"), "{stdout}");
    assert!(stdout.contains("network: shared"), "{stdout}");
    assert!(
        stdout.contains("TMPDIR, TEMP and TMP"),
        "the temp mapping is stated: {stdout}"
    );
    assert!(
        stdout.contains("home: "),
        "the sandbox HOME is stated: {stdout}"
    );
    assert!(stdout.contains("read-only"), "{stdout}");
}

#[test]
fn doctor_prints_only_the_description() {
    let env = Env::new();
    let output = env.cmd().arg("doctor").output().expect("run");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Nothing but the description: every line is a known part of it, so prose
    // that is not the policy cannot leak onto stdout.
    for line in stdout.lines() {
        let known = line == "sandbox policy"
            || (line.starts_with("    /") && line.trim_start() == line.trim())
            || [
                "enforcement: ",
                "network: ",
                "writable set:",
                "session temp: ",
                "home: ",
                "environment: ",
                "isolation: ",
                "read-only: ",
            ]
            .iter()
            .any(|prefix| line.trim_start().starts_with(prefix));
        assert!(known, "unexpected line in doctor output: {line:?}");
    }
}

fn hidden(omit: &[&str]) -> TempDir {
    path_without(&required_binaries(), omit, &common::host_path())
}

#[test]
fn doctor_exits_three_with_one_line_when_bwrap_is_missing() {
    let env = Env::new();
    let path = hidden(&["bwrap"]);
    env.cmd()
        .arg("doctor")
        .env("PATH", path.path())
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("bwrap")
                .and(predicate::str::contains("bubblewrap"))
                .and(predicate::function(|stderr: &str| {
                    stderr.lines().count() == 1
                })),
        );
}

#[test]
fn doctor_exits_three_with_one_line_when_a_required_binary_is_missing() {
    let env = Env::new();
    let path = hidden(&["yq"]);
    env.cmd()
        .arg("doctor")
        .env("PATH", path.path())
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("yq")
                .and(predicate::str::contains("sindri doctor"))
                .and(predicate::function(|stderr: &str| {
                    stderr.lines().count() == 1
                })),
        );
}

#[test]
fn doctor_exits_three_with_one_line_when_bwrap_cannot_create_a_sandbox() {
    let env = Env::new();
    let path = hidden(&["bwrap"]);
    let fake = path.path().join("bwrap");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho 'setting up uid map: Permission denied' >&2\nexit 1\n",
    )
    .expect("write fake bwrap");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");

    env.cmd()
        .arg("doctor")
        .env("PATH", path.path())
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("Permission denied")
                .and(predicate::str::contains("user namespaces"))
                .and(predicate::function(|stderr: &str| {
                    stderr.lines().count() == 1
                })),
        );
}

#[test]
fn a_refused_repository_config_is_announced_by_doctor_once() {
    let env = Env::new();
    std::fs::write(
        env.workspace.join(".sindri.toml"),
        "extra_writable_paths = [\"/\"]\n",
    )
    .expect("a committed config");

    // The diagnostic reads the config through the same door a run does, so the
    // note a refused repository config earns is said here too — once, on the
    // channel every front end says it on, and never as part of the description.
    let assert = env.cmd().arg("doctor").assert().success();
    let output = assert.get_output();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(
        stderr.matches(".sindri.toml").count(),
        1,
        "the refusal is said exactly once: {stderr}"
    );
    assert!(stderr.contains("ignor"), "and says what it did: {stderr}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("writable set"),
        "the description is still printed: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn a_config_a_run_would_refuse_is_refused_by_doctor_too() {
    let env = Env::new();
    common::write_config(&env, "max_turns = 0\n");

    env.cmd()
        .arg("doctor")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("max_turns").and(predicate::function(|stderr: &str| {
                stderr.lines().count() == 1
            })),
        );
}

#[test]
fn doctor_describes_the_configured_extras_the_model_is_told_about() {
    let env = Env::new();
    let extra = env.workspace.parent().expect("root").join("extra");
    std::fs::create_dir_all(&extra).expect("extra dir");
    let extra = extra.canonicalize().expect("canonical extra");
    common::write_config(
        &env,
        &format!("extra_writable_paths = [\"{}\"]\n", extra.display()),
    );

    let assert = env.cmd().arg("doctor").assert().success();
    let output = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert!(
        output.contains(&extra.display().to_string()),
        "doctor prints the description a run would really give the model: {output}"
    );
}

#[test]
fn doctor_with_the_flag_proves_no_sandbox_and_says_the_run_has_none() {
    let env = Env::new();
    // A machine where bwrap cannot even be found: the run will not use it, so
    // its absence is not this run's failure.
    let path = hidden(&["bwrap"]);
    let output = env
        .cmd()
        .args(["doctor", "--yolo-unsafe"])
        .env("PATH", path.path())
        .output()
        .expect("run");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "",
        "doctor is silent on success"
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("enforcement: none"), "{stdout}");
    assert!(stdout.contains("your own privileges"), "{stdout}");
    assert!(
        !stdout.contains("writable set"),
        "nothing bounds the run, so nothing is claimed to: {stdout}"
    );
}

#[test]
fn doctor_with_the_flag_still_reports_a_missing_binary() {
    let env = Env::new();
    let path = hidden(&["yq"]);
    env.cmd()
        .args(["doctor", "--yolo-unsafe"])
        .env("PATH", path.path())
        .assert()
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("yq").and(predicate::function(|stderr: &str| {
                stderr.lines().count() == 1
            })),
        );
}

#[test]
fn the_flag_does_not_excuse_the_writable_set_it_no_longer_bounds() {
    let env = Env::new();
    // Beside the workspace, never inside it: an extra nested in a path already
    // granted is refused, flag or no flag.
    let extra = TempDir::new().expect("an extra path");
    let extra = extra.path().to_path_buf();

    // Granted and validated, though nothing about the run is bounded by it.
    let output = env
        .cmd()
        // `--writable` is not global: it comes before the subcommand.
        .args([
            "--writable",
            &extra.display().to_string(),
            "doctor",
            "--yolo-unsafe",
        ])
        .output()
        .expect("run");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        !stdout.contains(&extra.display().to_string()),
        "the unsafe description claims no bound it does not enforce: {stdout}"
    );

    // A path that could not be mounted is a usage error under either policy.
    env.cmd()
        .args([
            "--writable",
            "/nonexistent-sindri-extra",
            "doctor",
            "--yolo-unsafe",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("/nonexistent-sindri-extra"));
}
