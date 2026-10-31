//! The suite's own contract: one table line per eval, and a status a CI job can
//! branch on.

#![allow(
    clippy::expect_used,
    reason = "a test harness reports a setup failure by panicking"
)]

use sindri_evals::report::{Verdict, exit_code, line};

fn verdict(name: &'static str, problems: &[&str]) -> Verdict {
    Verdict {
        name,
        problems: problems.iter().map(|p| (*p).to_string()).collect(),
    }
}

#[test]
fn a_passing_eval_is_one_line_that_says_pass() {
    let text = line(&verdict("1  summarise a diff", &[]));
    assert!(text.ends_with("PASS"), "{text}");
    assert_eq!(text.lines().count(), 1, "{text}");
}

#[test]
fn a_failing_eval_says_fail_and_why_on_the_same_line() {
    let text = line(&verdict(
        "7  the sandbox refuses a write",
        &["a probe escaped"],
    ));
    assert!(text.contains("FAIL"), "{text}");
    assert!(text.contains("a probe escaped"), "{text}");
    assert_eq!(text.lines().count(), 1, "{text}");
}

#[test]
fn the_status_is_non_zero_when_any_eval_failed() {
    let passing = verdict("1  summarise a diff", &[]);
    let failing = verdict("7  the sandbox refuses a write", &["a probe escaped"]);
    assert_eq!(exit_code(std::slice::from_ref(&passing)), 0);
    assert_eq!(exit_code(&[passing, failing]), 1);
}
