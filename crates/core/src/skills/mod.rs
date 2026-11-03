//! Skill discovery: a scan of the three scopes — the skills compiled into the
//! binary, the home directory's, and every repository directory the workspace
//! stands in — one fold into the catalog, the frontmatter read, and every file
//! that would not load kept as a diagnostic rather than dropped. A scope hands
//! back the found it read; the fold is what makes a catalog of them.
//!
//! The Agent Skills specification governs what is inside a skill directory and
//! says nothing about where one lives, so the paths here are ours: `~/.agents/skills`
//! for the user scope, and every `.agents/skills` between the workspace and its
//! repository root for the repo scope — the walk is how a run three crates deep in
//! a monorepo still finds the root's skills. A nearer scope wins a name: the
//! repository over the home directory, and both over a built-in, which is the
//! lowest. Every name that lost is recorded.
//! Nothing in this module reaches the model — a catalog is a thing Sindri can see
//! and say.

mod builtin;
mod catalog;
mod content;
mod frontmatter;
mod report;
mod scan;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::workspace;

pub use builtin::{BUILTINS, CONTENT_CEILING, FILE_CEILING, builtins};
pub use catalog::{Catalog, Content, File, Found, Origin, Refusal, Scope, Shadow, Skill, Where};
pub use frontmatter::DESCRIPTION_CEILING;
pub use scan::scan;

/// The file every skill directory holds, and the one the scan reads.
pub(crate) const SKILL: &str = "SKILL.md";

/// All three scopes: the repository's directories nearest first, then the home
/// directory, then the built-ins. The first skill to hold a name keeps it, which
/// is what makes the order the precedence: a repository's over the home
/// directory's, a crate's over its repository's, and both over a built-in.
pub fn discover(workspace: &Path, home: Option<&Path>) -> Catalog {
    discover_in(workspace, workspace::root(workspace).as_deref(), home)
}

/// All three scopes, with the repository root a caller has already probed handed
/// in: the rules, the skills and the git facts then hang off one root instead of
/// three definitions of "the repository root" that can disagree. `None` is a
/// workspace in no repository, which leaves the built-ins and the user scope.
pub fn discover_in(workspace: &Path, root: Option<&Path>, home: Option<&Path>) -> Catalog {
    let repos = root
        .map(|root| repo_scopes(workspace, root))
        .unwrap_or_default();
    fold(
        repos
            .into_iter()
            .map(|dir| scan(&dir, Scope::Repo))
            .chain(home.map(|home| scan(&scan::scope_dir(home), Scope::User)))
            // Last, and therefore the lowest: `fold` never replaces a name
            // already held, so a built-in is what every nearer scope takes a
            // name from.
            .chain(std::iter::once(builtins())),
    )
}

/// The one fold: every scope's found, in precedence order, into the catalog.
/// A name already held is not replaced — the order the scopes arrive in is the
/// precedence — and the skill that would have taken it is recorded, so a reader
/// sees both the winner and what it displaced. This is the only place a shadow
/// is made, and each found is read once: the name a skill carries is remembered
/// as it is taken, never searched for again.
pub fn fold(scopes: impl IntoIterator<Item = Found>) -> Catalog {
    let mut catalog = Catalog::default();
    let mut held: HashMap<String, usize> = HashMap::new();
    for found in scopes {
        // In the order the scopes were read, so the refusals beside them are too.
        catalog.refused.extend(found.refused);
        for skill in found.skills {
            match held.get(&skill.name) {
                Some(&winner) => catalog.shadowed.push(Shadow {
                    loser: skill.origin(),
                    winner: catalog.skills[winner].origin(),
                }),
                None => {
                    held.insert(skill.name.clone(), catalog.skills.len());
                    catalog.skills.push(skill);
                }
            }
        }
    }
    catalog.skills.sort_by(|one, two| one.name.cmp(&two.name));
    catalog
        .shadowed
        .sort_by(|one, two| one.winner.name.cmp(&two.winner.name));
    catalog
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
