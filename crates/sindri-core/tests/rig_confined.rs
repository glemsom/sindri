//! Proves rig is confined to one module tree by reading the source rather than
//! trusting a convention: every file under `src/` is scanned for the name, and
//! only the provider seam and the runtime module that will drive rig's runner
//! (ticket #17) may carry it.
//!
//! A name is read in code, not in prose: a line's `//` comment is dropped before
//! the scan, so a module may explain rig in words while never calling it. The
//! names counted are `rig`, `rig_core`, `rig_reqwest` and every other `rig_*`
//! or `rig-*` word, which is what an import of the library looks like.
//!
//! The positive half matters as much as the negative one: a tree where nothing
//! names rig would pass a purely negative test while the seam did nothing.

use std::fs;
use std::path::{Path, PathBuf};

/// The module tree allowed to name rig, as the path of its file and of its
/// directory, and the one runtime file §10 and ADR-0004 also allow.
const ALLOWED: [&str; 3] = ["src/provider.rs", "src/provider/", "src/run.rs"];

/// Whether `word` is rig's name: the bare crate or any crate of its family.
fn names_rig(word: &str) -> bool {
    word == "rig" || word.starts_with("rig_") || word.starts_with("rig-")
}

/// Every `rig` word in `source`, with `//` comments dropped first.
fn rig_words(source: &str) -> Vec<String> {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .flat_map(|code| {
            code.split(|character: char| !(character.is_alphanumeric() || character == '_'))
                .filter(|word| names_rig(word))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every Rust file under `root/src`, in a stable order.
fn sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.join("src")];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory).expect("the source tree is readable");
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Whether `path`, relative to the crate root, may name rig: the exact file, or
/// anything under an allowed directory.
fn allowed(path: &Path) -> bool {
    let relative = path.to_string_lossy().replace('\\', "/");
    ALLOWED
        .iter()
        .any(|permitted| relative == *permitted || relative.starts_with(permitted))
}

#[test]
fn only_the_provider_module_tree_names_rig() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    let mut seam_names_rig = false;

    for path in sources(root) {
        let source = fs::read_to_string(&path).expect("a source file is readable");
        let words = rig_words(&source);
        if words.is_empty() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .expect("a source file is under the crate root")
            .to_path_buf();
        if allowed(&relative) {
            seam_names_rig |= relative.starts_with("src/provider");
        } else {
            offenders.push(format!("{} names {words:?}", relative.display()));
        }
    }

    assert!(
        offenders.is_empty(),
        "only the provider module tree may name rig, but:\n  {}",
        offenders.join("\n  ")
    );
    assert!(
        seam_names_rig,
        "the provider seam is where rig is expected to be named, and nothing there names it"
    );
}
