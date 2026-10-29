//! The process seam for `--version`: the binary names its own source.

#![allow(
    clippy::expect_used,
    reason = "a panicking harness is how a process test reports a setup failure"
)]

mod common;

use common::Env;

fn git(args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8(out.stdout)
        .expect("git stdout")
        .trim()
        .to_string()
}

#[test]
fn version_names_the_git_describe_and_the_commit_date() {
    let env = Env::new();
    let assert = env.cmd().arg("--version").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    // An unchanged tree must not relink on rebuild, so a binary built from a
    // clean tree and tested on a dirty one keeps the values it was built with;
    // `-dirty` is the only part allowed to differ.
    let describe = git(&["describe", "--tags", "--always", "--dirty"]);
    let describe = describe.strip_prefix('v').unwrap_or(&describe);
    let pinned = describe.trim_end_matches("-dirty");
    let commit_date = git(&["log", "-1", "--format=%cs"]);

    assert!(stdout.starts_with("sindri "), "version output: {stdout:?}");
    assert!(
        stdout.contains(pinned),
        "version names the source revision {pinned}: {stdout:?}"
    );
    assert!(
        stdout.contains(&commit_date),
        "version names the commit date {commit_date}, never the wall clock: {stdout:?}"
    );
    assert!(stdout.ends_with('\n'), "one trailing newline");
}
