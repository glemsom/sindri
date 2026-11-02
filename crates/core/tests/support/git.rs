//! A repository a test can ask about: one commit, so a probe has a branch and a
//! commit to name. Git is a process, so it lives in its own fixture rather than
//! beside the ones that build values.

use std::path::Path;

/// A tree with one commit, so the probe has a branch and a commit to name.
pub fn commit(dir: &Path) -> String {
    let author = [
        "-c",
        "user.name=sindri",
        "-c",
        "user.email=sindri@example.invalid",
    ];
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(author)
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q", "--initial-branch=main"]);
    std::fs::write(dir.join("readme"), "one line\n").expect("a file to commit");
    git(&["add", "readme"]);
    git(&["commit", "-q", "-m", "the first commit"]);
    git(&["rev-parse", "HEAD"])
}
