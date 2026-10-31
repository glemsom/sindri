//! The suite's own shell contract: a selector that matches nothing is a usage
//! error, so a typo cannot be mistaken for a green run.

#![allow(
    clippy::expect_used,
    reason = "a test harness reports a setup failure by panicking"
)]

#[test]
fn a_selector_that_matches_no_eval_is_a_usage_error() {
    let assert = std::process::Command::new(env!("CARGO_BIN_EXE_sindri-evals"))
        .args(["--only", "zebra"])
        .output()
        .expect("run sindri-evals");
    assert_eq!(assert.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&assert.stderr).to_string();
    assert!(stderr.contains("no eval matches"), "{stderr}");
    assert!(
        String::from_utf8_lossy(&assert.stdout).is_empty(),
        "the table stays empty when nothing ran"
    );
}
