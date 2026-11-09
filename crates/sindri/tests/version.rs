//! The binary says which commit it was built at and that commit's date, both compile-time constants
//! with no clock near them, and the checkout is what it is held to: a version stamped from the wall
//! clock fails here, and a source tarball has no checkout to compare against, which the test says.

use std::path::Path;
use std::process::Command;

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

fn git_about_the_checkout() -> Option<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let describe = git(root, &["describe", "--tags", "--always", "--dirty"])?;
    let date = git(root, &["log", "-1", "--format=%cs"])?;
    Some((describe, date))
}

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
