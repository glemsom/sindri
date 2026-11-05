//! The suite's selector, proven without a binary, a sandbox or a recording: the
//! names are handed in, so what `--only` names is a value a test can read.

#![allow(
    clippy::expect_used,
    reason = "a test harness reports a setup failure by panicking"
)]

use sindri_evals::select::{Named, Refused, named};

/// Three names shaped the way the suite writes them: the eval's number, two
/// spaces, and the sentence it is known by — the pair `1` and `10` included,
/// because that is the pair a substring let through together.
const NAMES: &[&str] = &[
    "1  summarise a diff",
    "2  explain a file",
    "10 the house rules change the run",
];

#[test]
fn a_number_names_its_own_eval_and_never_the_one_it_prefixes() {
    assert_eq!(
        named(Some("1"), NAMES),
        Ok(Named::One("1  summarise a diff"))
    );
    assert_eq!(
        named(Some("10"), NAMES),
        Ok(Named::One("10 the house rules change the run"))
    );
}

#[test]
fn a_selector_that_names_several_evals_is_refused_and_names_each_one() {
    let refused = named(Some("a"), NAMES).expect_err("`a` is a word of two of them");
    assert_eq!(
        refused,
        Refused::Several {
            selector: "a",
            names: vec!["1  summarise a diff", "2  explain a file"],
        }
    );
    let said = refused.message();
    for name in ["1  summarise a diff", "2  explain a file"] {
        assert!(said.contains(name), "the refusal names {name:?}: {said}");
    }
    assert!(
        !said.contains("10 the house rules change the run"),
        "the refusal names only the evals it matched: {said}"
    );
}

#[test]
fn a_word_of_a_name_finds_the_eval_that_holds_it() {
    assert_eq!(
        named(Some("house"), NAMES),
        Ok(Named::One("10 the house rules change the run"))
    );
}

#[test]
fn a_word_no_name_holds_is_refused_with_the_line_the_shell_reads() {
    let refused = named(Some("zebra"), NAMES).expect_err("no eval is called zebra");
    assert_eq!(refused.message(), "sindri-evals: no eval matches `zebra`");
    let prefix = named(Some("summ"), NAMES).expect_err("half a word names nothing");
    assert_eq!(prefix.message(), "sindri-evals: no eval matches `summ`");
}

#[test]
fn no_selector_at_all_runs_the_whole_suite() {
    assert_eq!(named(None, NAMES), Ok(Named::Every));
}
