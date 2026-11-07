//! Where the binary says it came from: the `git describe` of the commit it was
//! built at and that commit's own date, handed to the crate as compile-time
//! constants rather than something the running binary could get wrong.
//!
//! Nothing here reads a clock, and nothing here writes the tree it reads. A build
//! script that stamped the time of the build would make every build a different
//! binary and an unchanged tree relink, which is the bug ticket #61 exists to
//! prevent; the commit's date is the only date a reproducible build carries. For
//! the same reason the dirty check is not `describe --dirty`: that command
//! refreshes the index, and the index is a file this script watches, so a build
//! that asked for it could relink itself for nothing. The comparison is made
//! instead by a `status --porcelain` that keeps its refresh in memory.
//!
//! What is watched is what genuinely changes the answer: the ref `HEAD` names, the
//! index, and the packed refs `describe` walks — never the sources, which `git
//! describe` cannot see. Watching them is what makes the answer stable: a build
//! with nothing committed between it and the last one re-runs this script, is told
//! the same thing, and relinks nothing.
//!
//! A tree with no git at all — a source tarball — still builds: the version falls
//! back to the package version, and the date says plainly that it knows none.

use std::path::PathBuf;
use std::process::{Command, Output};

/// The commit's date, said this way when the build stands on no commit at all.
const NO_COMMIT_DATE: &str = "unknown";

fn main() {
    watch_what_changes_the_answer();

    match (describe(), commit_date()) {
        (Some(describe), Some(date)) => {
            println!("cargo:rustc-env=SINDRI_DESCRIBE={describe}");
            println!("cargo:rustc-env=SINDRI_COMMIT_DATE={date}");
        }
        // No git, or git that will not answer: a source tarball is a legitimate
        // way to have this source, and it must build.
        _ => {
            println!("cargo:rustc-env=SINDRI_DESCRIBE={}", package_version());
            println!("cargo:rustc-env=SINDRI_COMMIT_DATE={NO_COMMIT_DATE}");
        }
    }
}

/// `git describe --tags --always`, with git's own `-dirty` suffix when the
/// working tree differs from `HEAD`.
///
/// The suffix is appended here rather than asked for as `--dirty` for one reason,
/// and it is not a stylistic one: `describe --dirty` refreshes the index while it
/// compares, whatever git is told about optional locks, and the index is a file
/// this script watches. A build script that rewrote its own input would make the
/// next build relink for nothing. `dirty()` makes the same comparison and writes
/// nothing, so this build reads the tree and leaves it exactly as it found it.
fn describe() -> Option<String> {
    let described = git(&["describe", "--tags", "--always"])?;
    if dirty() {
        return Some(format!("{described}-dirty"));
    }
    Some(described)
}

/// Whether the working tree differs from `HEAD` — the comparison `describe
/// --dirty` makes, untracked files aside.
///
/// `status` is asked rather than `diff-index --quiet` because only `status`
/// refreshes the index in memory before it compares. `diff-index` reads the
/// index's stat data as it stands, so a file that was merely `touch`ed — the very
/// move the no-relink proof makes — reads as modified while its contents are the
/// commit's, and the build would call a clean tree dirty. `--no-optional-locks`
/// keeps the refresh in memory, so the index, which this script watches, is left
/// exactly where it was. A git that will not answer leaves the tree looking
/// clean, which is the shape a source tarball takes anyway.
fn dirty() -> bool {
    let asked = run(&[
        "--no-optional-locks",
        "status",
        "--porcelain",
        "--untracked-files=no",
    ]);
    match asked {
        Some(output) => !output.status.success() || !output.stdout.is_empty(),
        None => false,
    }
}

/// The committer date of `HEAD`, short — the date the commit was made, which is
/// not the day this build happens to run.
fn commit_date() -> Option<String> {
    git(&["log", "-1", "--format=%cs"])
}

/// The package's own version, the fallback when there is no commit to describe.
fn package_version() -> String {
    std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| String::from("0.0.0"))
}

/// Ask cargo to re-run this script when, and only when, git's answer would move.
///
/// `git rev-parse --git-path` is what makes this right in a linked worktree,
/// where `.git` is a file pointing elsewhere: the paths come back where git
/// actually keeps them. A path that is not there is not watched, so a plain
/// repository is not made to look like a worktree or the reverse.
fn watch_what_changes_the_answer() {
    let mut watched: Vec<String> = ["HEAD", "index", "packed-refs"]
        .iter()
        .filter_map(|name| git_path(name))
        .collect();
    if let Some(head) = git(&["symbolic-ref", "--quiet", "HEAD"]) {
        watched.extend(git_path(&head));
    }
    for path in watched {
        println!("cargo:rerun-if-changed={path}");
    }
}

/// The absolute path git keeps `name` at, if anything is there.
fn git_path(name: &str) -> Option<String> {
    let path = PathBuf::from(git(&["rev-parse", "--git-path", name])?);
    let path = if path.is_absolute() {
        path
    } else {
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?).join(path)
    };
    Some(path.canonicalize().ok()?.display().to_string())
}

/// One git command, run where this package's manifest lives, trimmed; nothing
/// when git is absent, refuses, or says nothing.
fn git(args: &[&str]) -> Option<String> {
    let output = run(args)?;
    if !output.status.success() {
        return None;
    }
    let said = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!said.is_empty()).then_some(said)
}

/// One git command, run where this package's manifest lives; nothing only when
/// git could not be run at all — an exit status is an answer here, not a failure.
fn run(args: &[&str]) -> Option<Output> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")?;
    Command::new("git")
        .arg("-C")
        .arg(manifest)
        .args(args)
        .output()
        .ok()
}
