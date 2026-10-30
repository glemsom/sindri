//! Derives the build version from `git describe` and the commit date, so a
//! binary identifies its own source. Never the wall clock: a timestamp would
//! relink every crate on every build.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Cargo rebuilds every dependent whenever a build script re-runs, and it
    // dirties the crate on every build while a watched path is missing. So the
    // watch list is exactly the paths that exist and that can change the
    // answer: `HEAD`, the ref it names, tags, and packed refs. `.git/index` is
    // deliberately absent — every `git status` would rebuild the world.
    for path in git_watch_paths() {
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    let version = git(&["describe", "--tags", "--always", "--dirty"])
        .map(|v| v.strip_prefix('v').unwrap_or(&v).to_string())
        .unwrap_or_else(|| env("CARGO_PKG_VERSION"));
    let date = git(&["log", "-1", "--format=%cs"]).unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=SINDRI_VERSION={version}");
    println!("cargo:rustc-env=SINDRI_COMMIT_DATE={date}");
    println!("cargo:rustc-env=SINDRI_VERSION_LONG={version} ({date})");
}

fn env(key: &str) -> String {
    std::env::var(key).unwrap_or_default()
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn git_watch_paths() -> Vec<PathBuf> {
    let Some(dir) = repo_git_dir() else {
        return Vec::new();
    };
    let mut paths = vec![
        dir.join("HEAD"),
        dir.join("packed-refs"),
        dir.join("refs").join("tags"),
    ];
    if let Some(reference) = head_ref(&dir) {
        paths.push(reference);
    }
    paths
}

fn head_ref(git_dir: &std::path::Path) -> Option<PathBuf> {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let name = head.trim().strip_prefix("ref:")?.trim().to_string();
    Some(git_dir.join(name))
}

fn repo_git_dir() -> Option<PathBuf> {
    let mut cur = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = cur.join(".git");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !cur.pop() {
            return None;
        }
    }
}
