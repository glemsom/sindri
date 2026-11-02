//! The trees these tests are written against: a repository with a workspace
//! three crates deep in it, and one skill directory holding one `SKILL.md`.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A repository with a commit-less `.git` marker and a workspace three crates
/// deep inside it, which is where a run in this monorepo really boots.
pub fn repo() -> (TempDir, PathBuf) {
    let root = TempDir::new().expect("a repository");
    std::fs::create_dir(root.path().join(".git")).expect("a git marker");
    let workspace = root.path().join("crates/core/src");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    (root, workspace)
}

/// One skill directory holding the `SKILL.md` it is given.
pub fn skill(at: &Path, name: &str, body: &str) {
    let dir = at.join(name);
    std::fs::create_dir_all(&dir).expect("a skill directory");
    std::fs::write(dir.join("SKILL.md"), body).expect("a SKILL.md");
}
