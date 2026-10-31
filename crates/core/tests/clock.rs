#![allow(
    clippy::expect_used,
    reason = "a panicking test is how a broken clock reports itself"
)]

//! A record's date: one format, in the local zone, and the same one `sessions
//! list` prints.

use std::process::Command;

use sindri_core::clock::at;

/// Coreutils' answer for the same moment: an independent local-zone conversion,
/// so the test cannot be satisfied by repeating the code's own arithmetic.
fn coreutils(ms: u64) -> String {
    let secs = ms / 1_000;
    let output = Command::new("date")
        .args(["-d", &format!("@{secs}"), "+%Y-%m-%d %H:%M"])
        .output()
        .expect("run date");
    assert!(output.status.success(), "date refused @{secs}");
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

#[test]
fn a_record_is_dated_in_the_local_zone_the_way_the_system_dates_it() {
    for ms in [0, 1_700_000_000_000, 1_780_000_000_000] {
        assert_eq!(at(ms), coreutils(ms), "the moment {ms} ms");
    }
}
