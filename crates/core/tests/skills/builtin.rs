//! The third scope: a `SKILL.md` compiled into the binary, read with no
//! directory behind it, and displaced by a nearer scope that carries the same
//! name. Every tree here holds no skill of its own, so a name in the catalog
//! belongs to the binary or to the one directory the test wrote.

use sindri_core::skills::{Catalog, Scope, Skill, builtins, discover};
use tempfile::TempDir;

use crate::fixtures::{repo, skill};

/// The built-in this file is written about. The ticket ships one; a second
/// would only turn every name below into a list.
fn one() -> Skill {
    builtins()
        .skills
        .into_iter()
        .next()
        .expect("the binary ships a skill")
}

#[test]
fn a_builtin_skill_is_found_in_a_workspace_that_holds_no_skill_at_all() {
    let workspace = TempDir::new().expect("a workspace");
    let shipped = builtins();

    let catalog = discover(workspace.path(), None);

    let names: Vec<&str> = catalog
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    let shipped_names: Vec<&str> = shipped
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(
        names, shipped_names,
        "the binary's own are in every catalog"
    );
    for found in &catalog.skills {
        assert_eq!(found.scope, Scope::Builtin);
        assert!(
            found.dir().is_none(),
            "a built-in is the one skill with no directory: {found:?}"
        );
    }
    assert!(catalog.refused.is_empty(), "{catalog:?}");
}

#[test]
fn a_builtin_body_and_its_files_are_read_from_the_binary() {
    let found = one();

    let body = found.body().expect("the bytes are compiled in");
    assert!(!body.trim().is_empty());
    assert!(
        !body.starts_with("---") && !body.contains("\nname: "),
        "the frontmatter is stripped, as it is for any skill: {body}"
    );

    let listing = found.listing();
    assert!(
        !listing.is_empty(),
        "the files are listed from the same source"
    );
    assert_eq!(
        listing.len(),
        found.files().len(),
        "every file the body may name is listed: {listing:?}"
    );
    for file in found.files() {
        assert!(
            listing.contains(&file.path.to_string()),
            "{} is listed: {listing:?}",
            file.path
        );
        assert!(
            !file.text.trim().is_empty(),
            "{} is compiled in, so it can be read: {:?}",
            file.path,
            file.text
        );
    }
}

#[test]
fn every_builtin_skill_parses_and_names_a_file_its_body_points_at() {
    let shipped = builtins().skills;
    assert_eq!(
        shipped.len(),
        sindri_core::skills::BUILTINS.len(),
        "a built-in the frontmatter reader refused is skipped, so the count is what says none was"
    );
    for found in shipped {
        assert!(!found.description.trim().is_empty(), "{found:?}");
        let body = found.body().expect("a built-in's frontmatter parses");
        for file in found.files() {
            assert!(
                body.contains(file.path),
                "{} is a file nothing in the body names: {body}",
                file.path
            );
        }
    }
}

#[test]
fn two_builtins_never_ship_one_name_because_neither_can_shadow_the_other() {
    let shipped = builtins().skills;
    let mut names: Vec<&str> = shipped.iter().map(|skill| skill.name.as_str()).collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        total,
        "one built-in per name: a repeat inside one scope has no nearer skill to lose to, so \
         the fold would record it as its own loser and `sindri doctor` would print `builtin \
         shadows builtin`: {names:?}"
    );
}

#[test]
fn a_user_skill_of_a_builtins_name_displaces_it_and_the_shadow_names_what_lost() {
    let home = TempDir::new().expect("a home");
    let workspace = TempDir::new().expect("a workspace");
    let name = one().name;
    skill(
        &home.path().join(".agents/skills"),
        &name,
        &format!("---\nname: {name}\ndescription: The user's own.\n---\n\n# mine\n"),
    );

    let catalog: Catalog = discover(workspace.path(), Some(home.path()));

    let winner = catalog
        .skills
        .iter()
        .find(|skill| skill.name == name)
        .expect("the name is held");
    assert_eq!(winner.scope, Scope::User, "the nearer scope wins");
    assert_eq!(winner.description, "The user's own.");
    assert_eq!(catalog.shadowed.len(), 1, "{catalog:?}");
    let shadow = &catalog.shadowed[0];
    assert_eq!(shadow.loser.name, name);
    assert_eq!(shadow.loser.scope, Scope::Builtin);
    assert_eq!(
        shadow.loser.dir, None,
        "a built-in is not printed by a path it has not"
    );
    let report = catalog.report();
    assert!(
        report.contains(&format!("{name}: ")) && report.contains("shadows builtin"),
        "the displaced built-in is named with its origin: {report}"
    );
}

#[test]
fn a_repository_skill_takes_the_name_from_both_the_user_skill_and_the_builtin() {
    let home = TempDir::new().expect("a home");
    let (root, workspace) = repo();
    let name = one().name;
    skill(
        &home.path().join(".agents/skills"),
        &name,
        &format!("---\nname: {name}\ndescription: The user's own.\n---\n"),
    );
    skill(
        &root.path().join(".agents/skills"),
        &name,
        &format!("---\nname: {name}\ndescription: This repository's.\n---\n"),
    );

    let catalog = discover(&workspace, Some(home.path()));

    let winner = catalog
        .skills
        .iter()
        .find(|skill| skill.name == name)
        .expect("the name is held");
    assert_eq!(
        winner.scope,
        Scope::Repo,
        "nearest wins: repo, user, builtin"
    );
    assert_eq!(
        catalog.shadowed.len(),
        2,
        "both the losers are recorded: {catalog:?}"
    );
    let mut lost: Vec<Scope> = catalog.shadowed.iter().map(|one| one.loser.scope).collect();
    lost.sort_by_key(|scope| scope.as_str());
    assert_eq!(lost, vec![Scope::Builtin, Scope::User]);
    assert_eq!(
        catalog.shadowed[0].winner.scope,
        Scope::Repo,
        "and the winner is the one that holds it"
    );
}

#[test]
fn the_report_prints_a_builtins_own_scope_word_and_no_path_under_it() {
    let workspace = TempDir::new().expect("a workspace");

    let report = discover(workspace.path(), None).report();

    let row = report
        .lines()
        .find(|line| line.contains(&one().name))
        .expect("the built-in is a row of the report");
    assert!(row.starts_with("  builtin"), "its own scope word: {row}");
    assert!(
        !row.contains('/'),
        "no path is printed for a skill that has none: {row}"
    );
}
