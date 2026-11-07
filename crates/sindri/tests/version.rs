//! Proves the binary carries where it came from and says it out loud: the `git
//! describe` of the commit it was built at and that commit's own date, both
//! compile-time constants, printed by `--version` with no clock anywhere near them.
//!
//! The checkout is the independent source of truth here. The test asks git what
//! the tree stands on and demands the binary say the same thing, so a version
//! stamped from the wall clock — or one hand-written and forgotten — fails the day
//! the build is not the commit's day. A tree with no git at all, a source tarball,
//! has no such truth to compare against, and the test says so rather than passing
//! on a guess.

use std::path::Path;
use std::process::Command;

/// What the binary prints for `--version`, without its trailing newline.
fn printed_version() -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_sindri"))
        .arg("--version")
        .output()
        .expect("sindri --version should run");
    assert!(
        output.status.success(),
        "sindri --version exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8(output.stdout).expect("--version prints UTF-8");
    printed.trim_end_matches('\n').to_owned()
}

/// What git says about the checkout this test was built from, or nothing when
/// there is no checkout to ask: a source tarball carries no history.
fn git_about_the_checkout() -> Option<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let describe = git(root, &["describe", "--tags", "--always", "--dirty"])?;
    let date = git(root, &["log", "-1", "--format=%cs"])?;
    Some((describe, date))
}

/// One git command, in `root`, trimmed; nothing when it fails or prints nothing.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let said = String::from_utf8(output.stdout).ok()?;
    let said = said.trim().to_owned();
    (!said.is_empty()).then_some(said)
}

/// Today, by the clock the test process runs under — the one date a reproducible
/// build is forbidden to carry.
fn today() -> Option<String> {
    let output = Command::new("date").arg("-u").arg("+%F").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let said = String::from_utf8(output.stdout).ok()?;
    Some(said.trim().to_owned())
}

#[test]
fn version_line_repeats_the_describe_and_commit_date_git_reports() {
    let Some((describe, commit_date)) = git_about_the_checkout() else {
        eprintln!("no git checkout to compare against: this is a source tarball");
        return;
    };

    let printed = printed_version();
    assert!(
        printed.contains(&describe),
        "the version line '{printed}' does not carry git describe '{describe}'"
    );
    assert!(
        printed.contains(&format!("({commit_date})")),
        "the version line '{printed}' does not carry the commit date '{commit_date}'"
    );
}

#[test]
fn version_line_carries_the_commit_date_and_not_the_day_it_was_built() {
    let Some((_, commit_date)) = git_about_the_checkout() else {
        eprintln!("no git checkout to compare against: this is a source tarball");
        return;
    };
    let Some(today) = today() else {
        eprintln!("no date(1) to read the wall clock with");
        return;
    };
    if today == commit_date {
        eprintln!("the commit was made today, so the clock and the commit agree");
        return;
    }

    let printed = printed_version();
    assert!(
        !printed.contains(&format!("({today})")),
        "the version line '{printed}' carries the wall clock's {today} rather than the commit's {commit_date}"
    );
}
