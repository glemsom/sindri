//! A session id is the kernel's UUID, held to the same guard a lookup runs: the
//! shape the record leans on is asserted here rather than assumed, and a name
//! that cannot be read answers no id instead of an empty lookup.

#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken identifier reports itself"
)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use sindri_core::paths::{SessionDir, is_session_id, new_session_id};
use tempfile::TempDir;

/// The lengths of a UUID's five runs: `8-4-4-4-12`.
const RUNS: [usize; 5] = [8, 4, 4, 4, 12];

fn uuid_shaped(id: &str) -> bool {
    let runs: Vec<&str> = id.split('-').collect();
    runs.len() == RUNS.len()
        && runs
            .iter()
            .zip(RUNS)
            .all(|(run, len)| run.len() == len && run.chars().all(|c| c.is_ascii_hexdigit()))
}

#[test]
fn a_session_id_is_a_uuid_and_never_a_name_a_lookup_could_climb() {
    let first = new_session_id().expect("the kernel answers a UUID");
    let second = new_session_id().expect("the kernel answers a UUID");
    assert_ne!(first, second, "two calls are two ids");
    for id in [&first, &second] {
        assert!(uuid_shaped(id), "{id} is 8-4-4-4-12 hex and dashes");
        assert!(is_session_id(id), "{id} is a name the record guard accepts");
        assert!(!id.contains('/'), "{id} is not a path");
    }

    let state = TempDir::new().expect("a state directory");
    let dir = SessionDir::create_in(state.path()).expect("a session directory");
    assert!(
        is_session_id(dir.id()),
        "a directory under {} answers an id the guard accepts, not {:?}",
        state.path().display(),
        dir.id()
    );
}

#[test]
fn a_directory_named_in_bytes_that_are_not_utf8_answers_no_id() {
    let state = TempDir::new().expect("a state directory");
    let dir = SessionDir::at(state.path().join(OsStr::from_bytes(b"\xff\xfe")));
    std::fs::create_dir_all(dir.root()).expect("a directory named in bytes");
    assert_eq!(dir.id(), "", "a name that is not UTF-8 is no id at all");
}
