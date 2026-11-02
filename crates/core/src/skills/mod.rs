//! Skill discovery: the two scopes scanned, the frontmatter read, and every
//! file that would not load kept as a diagnostic rather than dropped.
//!
//! The Agent Skills specification governs what is inside a skill directory and
//! says nothing about where one lives, so the paths here are ours: `~/.agents/skills`
//! for the user scope, and every `.agents/skills` between the workspace and its
//! repository root for the repo scope — the walk is how a run three crates deep in
//! a monorepo still finds the root's skills. A nearer scope wins a name, the
//! repository wins over the home directory, and every name that lost is recorded.
//! Nothing in this module reaches the model — a catalog is a thing Sindri can see
//! and say.

mod catalog;
mod content;
mod frontmatter;
mod report;
mod scan;

use std::path::{Path, PathBuf};

use crate::workspace;

pub use catalog::{Catalog, Refusal, Scope, Shadow, Skill};

/// The file every skill directory holds, and the one the scan reads.
pub(crate) const SKILL: &str = "SKILL.md";

/// Both scopes. The repository's directories are folded in nearest first and the
/// home directory last of all, so the first skill to hold a name keeps it: a
/// repository's over the home directory's, a crate's over its repository's.
pub fn discover(workspace: &Path, home: Option<&Path>) -> Catalog {
    discover_in(workspace, workspace::root(workspace).as_deref(), home)
}

/// Both scopes, with the repository root a caller has already probed handed in:
/// the rules, the skills and the git facts then hang off one root instead of
/// three definitions of "the repository root" that can disagree. `None` is a
/// workspace in no repository, and leaves the user scope as the only one.
pub fn discover_in(workspace: &Path, root: Option<&Path>, home: Option<&Path>) -> Catalog {
    let mut catalog = Catalog::default();
    for dir in root
        .map(|root| repo_scopes(workspace, root))
        .unwrap_or_default()
    {
        absorb(&mut catalog, scan::scan(&dir, Scope::Repo));
    }
    if let Some(home) = home {
        absorb(
            &mut catalog,
            scan::scan(&scan::scope_dir(home), Scope::User),
        );
    }
    catalog.skills.sort_by(|one, two| one.name.cmp(&two.name));
    catalog.shadowed.sort_by(|one, two| one.name.cmp(&two.name));
    catalog
}

/// Folds one scope's scan in. A name already held is not replaced — the scan
/// order is the precedence — and the skill that would have taken it is recorded,
/// so a reader can see both the winner and what it displaced.
fn absorb(into: &mut Catalog, scope: Catalog) {
    for skill in scope.skills {
        match into.skills.iter().find(|held| held.name == skill.name) {
            Some(held) => into.shadowed.push(Shadow {
                name: skill.name,
                loser: skill.dir,
                winner: held.dir.clone(),
            }),
            None => into.skills.push(skill),
        }
    }
    into.refused.extend(scope.refused);
}

/// The repository's skill directories, nearest first, and never one above the
/// root it was given: a directory above it belongs to somebody else's repository.
/// Every one of them is a scope, so a crate's own skills do not hide the ones the
/// repository root carries for everybody.
fn repo_scopes(workspace: &Path, root: &Path) -> Vec<PathBuf> {
    workspace
        .ancestors()
        .take_while(|dir| dir.starts_with(root))
        .map(scan::scope_dir)
        .filter(|dir| dir.is_dir())
        .collect()
}
