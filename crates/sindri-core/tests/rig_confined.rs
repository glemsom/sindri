//! Proves rig is confined to one module tree by reading the source, not by trusting a convention:
//! every file under `src/` is scanned, only the provider seam may carry the name, and comments are
//! dropped so a module may explain rig without calling it.

use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED: [&str; 2] = ["src/provider.rs", "src/provider/"];

fn names_rig(word: &str) -> bool {
    word == "rig" || word.starts_with("rig_") || word.starts_with("rig-")
}

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
