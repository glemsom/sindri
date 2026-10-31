//! The XDG rule, run on values instead of on the process environment: the
//! empty case is what the helper exists for, and a test that set `XDG_*_HOME`
//! would write the process every other test shares.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken rule reports itself"
)]

use std::ffi::OsString;
use std::path::PathBuf;

use super::under_from;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("the test needs a HOME to fall back to"))
}

#[test]
fn a_set_variable_roots_a_sindri_leaf_and_ignores_home() {
    assert_eq!(
        under_from(
            Some(OsString::from("/xdg/state")),
            "XDG_STATE_HOME",
            ".local/state/sindri"
        )
        .expect("a set variable is a directory"),
        PathBuf::from("/xdg/state/sindri")
    );
}

#[test]
fn an_empty_variable_falls_back_under_home() {
    assert_eq!(
        under_from(
            Some(OsString::new()),
            "XDG_STATE_HOME",
            ".local/state/sindri"
        )
        .expect("HOME answers for an empty variable"),
        home().join(".local/state/sindri")
    );
}

#[test]
fn an_unset_variable_falls_back_under_home() {
    assert_eq!(
        under_from(None, "XDG_CONFIG_HOME", ".config/sindri")
            .expect("HOME answers for an unset variable"),
        home().join(".config/sindri")
    );
}
