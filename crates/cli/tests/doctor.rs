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
