//! The skills the binary ships: a `SKILL.md` compiled in rather than installed
//! to a path, so the instructions a run is given and the binary that enforces
//! them ship as one artefact or neither does. Nothing is written at boot: there
//! is no directory to be absent, stale or unwritable, and no second copy of
//! ours under a path a model may also `cat`.
//!
//! The list is the third scope's found, handed to the fold after the two a scan
//! reads, and that order is where the precedence comes from: a name already held
//! is not replaced, so a built-in is the lowest of the three and a nearer scope's
//! skill takes its name.

use super::catalog::{Content, File, Found, Refusal, Scope, Skill, Where};
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

/// The built-in scope's found, handed to the fold after the two scopes a
/// scan reads, and therefore the lowest of the three.
///
/// A built-in that will not read is a refusal naming the scope that carries it,
/// because a compiled skill has no file: the rest of the catalog still holds, so
/// one broken `SKILL.md` costs the model that one skill and never the binary's
/// others.
pub fn builtins() -> Found {
    known(BUILTINS)
}

/// One table read into a found: the frontmatter of each entry by the same reader
/// a directory's copy goes through, so a built-in and a scanned skill are
/// assembled by one path rather than two.
fn known(table: &[Content]) -> Found {
    let mut found = Found::default();
    for content in table {
        let Content::Compiled { source, .. } = content else {
            continue;
        };
        match frontmatter::parse(source) {
            Ok(fields) => found
                .skills
                .push(Skill::new(fields, Scope::Builtin, content.clone())),
            Err(problem) => found.refused.push(Refusal {
                at: Where::Scope(Scope::Builtin),
                line: Some(problem.line),
                reason: problem.reason,
            }),
        }
    }
    found
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken contract reports itself"
    )]

    use super::{Content, File, Scope, Where, known};

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
        let found = known(BROKEN);

        assert!(found.skills.is_empty(), "no name is invented for it");
        assert_eq!(found.refused.len(), 1, "{found:?}");
        let refusal = &found.refused[0];
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
        let reason = refusal.reason.clone();
        // The built-ins folded on their own, which is the catalog the doctor
        // reads when they are the only scope a tree carries.
        assert_eq!(
            crate::skills::fold([found]).report(),
            format!("skills: none found\nskills: 1 refused\n  builtin:1: {reason}\n"),
            "`sindri doctor` prints the scope's word where a path would go"
        );
    }

    #[test]
    fn a_broken_builtin_leaves_the_rest_of_the_catalog_standing() {
        let table: Vec<Content> = BROKEN.iter().chain(SOUND).cloned().collect();

        let found = known(&table);

        assert_eq!(
            found.skills.len(),
            1,
            "the sound one still loads: {found:?}"
        );
        assert_eq!(found.skills[0].name, "sound");
        assert_eq!(found.skills[0].scope, Scope::Builtin);
        assert_eq!(found.refused.len(), 1, "{found:?}");
    }

    #[test]
    fn a_builtin_that_reads_is_a_compiled_skill_with_its_own_files() {
        let found = known(SOUND);

        assert!(found.refused.is_empty(), "{found:?}");
        let sound = &found.skills[0];
        assert!(sound.dir().is_none(), "a built-in is found in the binary");
        assert_eq!(
            sound.files().len(),
            1,
            "the files the table carries go with the body"
        );
        assert_eq!(
            sound.body().expect("the body is under the fence"),
            "# Sound"
        );
    }
}
