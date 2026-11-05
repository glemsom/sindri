//! The suite's own shell contract: a selector that names nothing or names
//! several evals is a usage error before anything runs, so a typo cannot be
//! mistaken for a green run and a narrowed suite cannot quietly grow.

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

#[test]
fn a_selector_that_names_several_evals_is_refused_before_anything_runs() {
    let assert = std::process::Command::new(env!("CARGO_BIN_EXE_sindri-evals"))
        .args(["--only", "the"])
        .output()
        .expect("run sindri-evals");
    assert_eq!(assert.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&assert.stderr).to_string();
    assert!(
        stderr.contains("9  the catalog triggers a skill"),
        "{stderr}"
    );
    assert!(
        stderr.contains("10 the house rules change the run"),
        "{stderr}"
    );
    assert!(
        String::from_utf8_lossy(&assert.stdout).is_empty(),
        "the table stays empty when nothing ran"
    );
}
