//! The trees these tests are written against: a repository with a workspace
//! three crates deep in it, and one skill directory holding one `SKILL.md`.

use std::path::{Path, PathBuf};

use sindri_core::skills::{Catalog, Scope, discover};
use tempfile::TempDir;

/// A repository with nothing committed and a workspace three crates deep inside
/// it, which is where a run in this monorepo really boots.
pub fn repo() -> (TempDir, PathBuf) {
    let root = TempDir::new().expect("a repository");
    sindri_harness::git::init(root.path());
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

/// The catalog of one tree with the skills the binary ships taken out. A test in
/// this module reads what a directory holds, and the built-ins are in every tree
/// alike; the ones that are about them read `discover` whole, in `builtin.rs`.
pub fn scanned(workspace: &Path, home: Option<&Path>) -> Catalog {
    let mut catalog = discover(workspace, home);
    catalog.skills.retain(|skill| skill.scope != Scope::Builtin);
    catalog
}
