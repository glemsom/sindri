//! The skills the binary ships: a `SKILL.md` compiled in rather than installed
//! to a path, so the instructions a run is given and the binary that enforces
//! them ship as one artefact or neither does. Nothing is written at boot: there
//! is no directory to be absent, stale or unwritable, and no second copy of
//! ours under a path a model may also `cat`.
//!
//! The list is folded into every catalog after the two scopes a scan reads, and
//! that order is where the precedence comes from: a name already held is not
//! replaced, so a built-in is the lowest of the three and a nearer scope's skill
//! takes its name.

use super::catalog::{Catalog, Content, File, Scope, Skill};
use super::frontmatter;

/// One built-in: its whole `SKILL.md`, and every file its body may name,
/// compiled in beside it.
pub struct Builtin {
    /// The `SKILL.md` the two required fields, the two invocation flags and the
    /// body are read from — by the same reader a directory's copy goes through.
    pub source: &'static str,
    /// The files the body may name, each with its own text, so a reference
    /// resolves where it is written and never to a path that is not there.
    pub files: &'static [File],
}

/// Every built-in skill. One table, no directory and no path: a scope is a place
/// a skill is found, and a compiled one is found in the binary.
pub const BUILTINS: &[Builtin] = &[Builtin {
    source: include_str!("builtin/SKILL.md"),
    files: &[
        File {
            path: "references/refusals.md",
            text: include_str!("builtin/references/refusals.md"),
        },
        File {
            path: "references/worked-example.md",
            text: include_str!("builtin/references/worked-example.md"),
        },
    ],
}];

/// The built-in scope as a catalog, folded in after the two scopes a scan
/// reads, and therefore the lowest.
///
/// A built-in whose frontmatter will not parse is skipped rather than refused,
/// because a refusal names the file it is about and this one has no file: the
/// test that reads every built-in is where that fault surfaces, not a run.
pub fn builtins() -> Catalog {
    let mut catalog = Catalog::default();
    for builtin in BUILTINS {
        let Ok(read) = frontmatter::parse(builtin.source) else {
            continue;
        };
        catalog.skills.push(Skill::new(
            read,
            Scope::Builtin,
            Content::Compiled {
                source: builtin.source,
                files: builtin.files,
            },
        ));
    }
    catalog
}
