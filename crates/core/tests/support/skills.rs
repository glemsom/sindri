//! A workspace with skills in it, for the tests about the `skill` tool: the
//! directories a scan reads, and the catalog that comes out of one.

use std::path::{Path, PathBuf};

use sindri_core::skills::{Catalog, Scope, discover};

/// One skill directory under `workspace`, holding a `SKILL.md` with `fields`
/// under `name` and `body` below the fence. Returns the directory.
pub fn write(workspace: &Path, name: &str, fields: &str, body: &str) -> PathBuf {
    // The repository root is where a scan stops, so a workspace with skills in
    // it is a workspace inside a repository — and one git will answer for, since
    // the root a scan stops at is the root git names.
    sindri_harness::git::init(workspace);
    let dir = workspace.join(".agents/skills").join(name);
    std::fs::create_dir_all(&dir).expect("skill directory");
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: what {name} is for\n{fields}---\n\n{body}\n"),
    )
    .expect("SKILL.md");
    dir
}

/// A skill with a file beside its body, so a result's listing has something a
/// relative reference in the body could resolve to.
pub fn reference(dir: &Path, path: &str, text: &str) {
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().expect("a parent")).expect("reference directory");
    std::fs::write(file, text).expect("reference");
}

/// The catalog the workspace holds, with no user scope: a test's collection is
/// its own and never the host's — and with the skills the binary ships taken
/// out, since these tests are about what a directory holds.
pub fn catalog(workspace: &Path) -> Catalog {
    let mut catalog = discover(workspace, None);
    catalog.skills.retain(|skill| skill.scope != Scope::Builtin);
    catalog
}
