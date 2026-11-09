//! The `git describe` of the commit this was built at, and that commit's own date, handed
//! to the crate as compile-time constants. Nothing here reads a clock or writes the tree it
//! reads, so an unchanged tree builds to the same bytes and relinks nothing.

use std::path::PathBuf;
use std::process::{Command, Output};

const NO_COMMIT_DATE: &str = "unknown";

fn main() {
    watch_what_changes_the_answer();

    match (describe(), commit_date()) {
        (Some(describe), Some(date)) => {
            println!("cargo:rustc-env=SINDRI_DESCRIBE={describe}");
            println!("cargo:rustc-env=SINDRI_COMMIT_DATE={date}");
        }
        // No git, or git that will not answer: a source tarball must build too.
        _ => {
            println!("cargo:rustc-env=SINDRI_DESCRIBE={}", package_version());
            println!("cargo:rustc-env=SINDRI_COMMIT_DATE={NO_COMMIT_DATE}");
        }
    }
}

/// `git describe --tags --always`, with a `-dirty` suffix this script appends itself:
/// asking git for `--dirty` would rewrite the index the build watches.
fn describe() -> Option<String> {
    let described = git(&["describe", "--tags", "--always"])?;
    if dirty() {
        return Some(format!("{described}-dirty"));
    }
    Some(described)
}

/// `status` and not `diff-index --quiet`: only `status` refreshes the index in memory
/// first, so a file merely `touch`ed does not read as modified while its contents are the
/// commit's. `--no-optional-locks` keeps that refresh from writing the index this watches.
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

fn commit_date() -> Option<String> {
    git(&["log", "-1", "--format=%cs"])
}

fn package_version() -> String {
    std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| String::from("0.0.0"))
}

/// `rev-parse --git-path` puts these where git really keeps them in a linked worktree,
/// where `.git` is a file; a path that is not there is not watched.
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

fn git_path(name: &str) -> Option<String> {
    let path = PathBuf::from(git(&["rev-parse", "--git-path", name])?);
    let path = if path.is_absolute() {
        path
    } else {
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?).join(path)
    };
    Some(path.canonicalize().ok()?.display().to_string())
}

fn git(args: &[&str]) -> Option<String> {
    let output = run(args)?;
    if !output.status.success() {
        return None;
    }
    let said = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!said.is_empty()).then_some(said)
}

/// One git command; nothing only when git could not be run at all — an exit status is an
/// answer here, not a failure.
fn run(args: &[&str]) -> Option<Output> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")?;
    Command::new("git")
        .arg("-C")
        .arg(manifest)
        .args(args)
        .output()
        .ok()
}
