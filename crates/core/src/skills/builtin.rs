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

use super::catalog::{Catalog, Content, File, Refusal, Scope, Skill, Where};
use super::frontmatter;

/// Every built-in skill: the one table, holding what a compiled skill already is
/// — its whole `SKILL.md`, and every file its body may name, compiled in beside
/// it. One table, no directory and no path: a scope is a place a skill is found,
/// and a compiled one is found in the binary.
///
/// A directory's copy cannot be written here even by mistake: a `PathBuf` is not
/// a thing a `const` can hold, so every entry is by construction a compiled one.
pub const BUILTINS: &[Content] = &[Content::Compiled {
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
/// A built-in that will not read is a refusal naming the scope that carries it,
/// because a compiled skill has no file: the rest of the catalog still holds, so
/// one broken `SKILL.md` costs the model that one skill and never the binary's
/// others.
pub fn builtins() -> Catalog {
    folded(BUILTINS)
}

/// A table folded into a catalog: the frontmatter of each entry by the same
/// reader a directory's copy goes through, so a built-in and a scanned skill are
/// assembled by one path rather than two.
fn folded(table: &[Content]) -> Catalog {
    let mut catalog = Catalog::default();
    for content in table {
        let Content::Compiled { source, .. } = content else {
            continue;
        };
        match frontmatter::parse(source) {
            Ok(fields) => catalog
                .skills
                .push(Skill::new(fields, Scope::Builtin, content.clone())),
            Err(problem) => catalog.refused.push(Refusal {
                at: Where::Scope(Scope::Builtin),
                line: Some(problem.line),
                reason: problem.reason,
            }),
        }
    }
    catalog
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken contract reports itself"
    )]

    use super::{Content, File, Scope, Where, folded};

    /// One skill with no description, which is the reader's refusal to make.
    const BROKEN: &[Content] = &[Content::Compiled {
        source: "---\nname: broken\n---\n\n# Broken\n",
        files: &[],
    }];

    /// One skill that reads, so the catalog beside a refusal can be measured.
    const SOUND: &[Content] = &[Content::Compiled {
        source: "---\nname: sound\ndescription: Reads the way a skill should.\n---\n\n# Sound\n",
        files: &[File {
            path: "references/one.md",
            text: "# One\n",
        }],
    }];

    #[test]
    fn a_builtin_that_will_not_read_is_a_refusal_naming_the_scope_that_carries_it() {
        let catalog = folded(BROKEN);

        assert!(catalog.skills.is_empty(), "no name is invented for it");
        assert_eq!(catalog.refused.len(), 1, "{catalog:?}");
        let refusal = &catalog.refused[0];
        assert_eq!(
            refusal.at,
            Where::Scope(Scope::Builtin),
            "a built-in has no file, so the scope that carries it is what is named"
        );
        assert_eq!(refusal.line, Some(1), "and the line that broke it");
        assert!(
            refusal.reason.contains("description"),
            "the reason is the reader's own: {refusal:?}"
        );
        assert_eq!(
            catalog.report(),
            format!(
                "skills: none found\nskills: 1 refused\n  builtin:1: {}\n",
                refusal.reason
            ),
            "`sindri doctor` prints the scope's word where a path would go"
        );
    }

    #[test]
    fn a_broken_builtin_leaves_the_rest_of_the_catalog_standing() {
        let table: Vec<Content> = BROKEN.iter().chain(SOUND).cloned().collect();

        let catalog = folded(&table);

        assert_eq!(
            catalog.skills.len(),
            1,
            "the sound one still loads: {catalog:?}"
        );
        assert_eq!(catalog.skills[0].name, "sound");
        assert_eq!(catalog.skills[0].scope, Scope::Builtin);
        assert_eq!(catalog.refused.len(), 1, "{catalog:?}");
    }

    #[test]
    fn a_builtin_that_reads_is_a_compiled_skill_with_its_own_files() {
        let catalog = folded(SOUND);

        assert!(catalog.refused.is_empty(), "{catalog:?}");
        let found = &catalog.skills[0];
        assert!(found.dir().is_none(), "a built-in is found in the binary");
        assert_eq!(
            found.files().len(),
            1,
            "the files the table carries go with the body"
        );
        assert_eq!(
            found.body().expect("the body is under the fence"),
            "# Sound"
        );
    }
}
