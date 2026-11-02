//! Both scopes, the shadowing between them, and the shapes of the tree a scan
//! has to survive: a link, a broken link, a directory that is not a skill.

use sindri_core::skills::{Origin, Scope};
use tempfile::TempDir;

use crate::fixtures::{repo, scanned, skill};

#[test]
fn a_repo_skill_is_found_with_its_name_description_and_flags() {
    let (root, workspace) = repo();
    skill(
        &root.path().join(".agents/skills"),
        "house-style",
        "---\nname: house-style\ndescription: Short sentences, and no adverb goes unpunished.\n---\n\n# House style\n",
    );

    let catalog = scanned(&workspace, None);

    assert_eq!(catalog.skills.len(), 1, "one skill, found: {catalog:?}");
    let found = &catalog.skills[0];
    assert_eq!(found.name, "house-style");
    assert_eq!(
        found.description,
        "Short sentences, and no adverb goes unpunished."
    );
    assert_eq!(found.scope, Scope::Repo, "it came from the repository");
    assert!(found.model_invocable, "nothing disabled the model");
    assert!(found.user_invocable, "nothing disabled the user");
    assert!(catalog.refused.is_empty(), "{catalog:?}");
}

#[test]
fn a_user_skill_is_found_and_says_so() {
    let home = TempDir::new().expect("a home");
    let workspace = TempDir::new().expect("a workspace");
    skill(
        &home.path().join(".agents/skills"),
        "grilling",
        "---\nname: grilling\ndescription: Ask until the plan stops wobbling.\n---\n",
    );

    let catalog = scanned(workspace.path(), Some(home.path()));

    assert_eq!(catalog.skills.len(), 1, "{catalog:?}");
    assert_eq!(catalog.skills[0].name, "grilling");
    assert_eq!(catalog.skills[0].scope, Scope::User);
}

#[test]
fn both_scopes_are_read_and_a_repo_skill_displaces_the_user_one_of_that_name() {
    let home = TempDir::new().expect("a home");
    let (root, workspace) = repo();
    skill(
        &home.path().join(".agents/skills"),
        "house-style",
        "---\nname: house-style\ndescription: The user's own house style.\n---\n",
    );
    skill(
        &home.path().join(".agents/skills"),
        "research",
        "---\nname: research\ndescription: Read primary sources.\n---\n",
    );
    skill(
        &root.path().join(".agents/skills"),
        "house-style",
        "---\nname: house-style\ndescription: This repository's house style.\n---\n",
    );

    let catalog = scanned(&workspace, Some(home.path()));

    let names: Vec<&str> = catalog
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(names, vec!["house-style", "research"], "sorted by name");
    let winner = &catalog.skills[0];
    assert_eq!(
        winner.description, "This repository's house style.",
        "the repository is what the run is about"
    );
    assert_eq!(winner.scope, Scope::Repo);
    assert_eq!(catalog.shadowed.len(), 1, "the shadowing is recorded");
    let shadow = &catalog.shadowed[0];
    assert_eq!(shadow.winner.name, "house-style");
    assert_eq!(
        shadow.loser,
        Origin {
            scope: Scope::User,
            name: "house-style".to_string(),
            dir: Some(home.path().join(".agents/skills/house-style")),
        }
    );
    assert_eq!(
        shadow.winner,
        Origin {
            scope: Scope::Repo,
            name: "house-style".to_string(),
            dir: Some(root.path().join(".agents/skills/house-style")),
        }
    );
}

#[test]
fn the_catalog_names_each_skill_its_scope_and_who_may_invoke_it() {
    let home = TempDir::new().expect("a home");
    let (root, workspace) = repo();
    skill(
        &home.path().join(".agents/skills"),
        "research",
        "---\nname: research\ndescription: Read primary sources.\n---\n",
    );
    skill(
        &root.path().join(".agents/skills"),
        "pr",
        "---\nname: pr\ndescription: Use when writing a PR body.\nuser-invocable: false\n---\n",
    );

    let report = scanned(&workspace, Some(home.path())).report();

    assert!(report.starts_with("skills: 2 found\n"), "{report}");
    assert!(
        report.contains("user") && report.contains("model,user"),
        "the user skill says who may invoke it: {report}"
    );
    assert!(
        report.contains("repo") && report.contains("model "),
        "the repo skill says the user may not: {report}"
    );
    assert!(
        report.ends_with('\n') && !report.ends_with("\n\n"),
        "{report}"
    );
}

#[test]
fn an_empty_catalog_says_so_rather_than_saying_nothing() {
    let workspace = TempDir::new().expect("a workspace");
    assert_eq!(
        scanned(workspace.path(), None).report(),
        "skills: none found\n"
    );
}

/// A skill directory, written somewhere else and shared into a scope by link:
/// how people keep one collection and use it in every repo.
#[test]
fn a_symlinked_skill_directory_is_followed_and_a_broken_link_is_a_diagnostic() {
    let (root, workspace) = repo();
    let shared = root.path().join("shared-skills/shared");
    std::fs::create_dir_all(&shared).expect("the shared skill");
    std::fs::write(
        shared.join("SKILL.md"),
        "---\nname: shared\ndescription: One copy, used everywhere.\n---\n",
    )
    .expect("a SKILL.md");
    let scope = root.path().join(".agents/skills");
    std::fs::create_dir_all(&scope).expect("the scope");
    std::os::unix::fs::symlink(&shared, scope.join("shared")).expect("a link to a skill");
    std::os::unix::fs::symlink(root.path().join("gone"), scope.join("gone"))
        .expect("a link to nothing");

    let catalog = scanned(&workspace, None);

    assert_eq!(catalog.skills.len(), 1, "{catalog:?}");
    assert_eq!(catalog.skills[0].name, "shared");
    assert_eq!(catalog.refused.len(), 1, "a broken link is said, not fatal");
    assert_eq!(catalog.refused[0].path, scope.join("gone"));
    assert_eq!(catalog.refused[0].line, None, "a link has no line");
    let target = root.path().join("gone");
    let named = format!("a broken link to {}: ", target.display());
    let reason = &catalog.refused[0].reason;
    assert!(
        reason.starts_with(&named) && reason.len() > named.len(),
        "the target that was read is named, with the error beside it: {reason}"
    );
}

#[test]
fn the_scan_is_one_level_deep_so_only_directories_holding_a_skill_md_are_skills() {
    let (root, workspace) = repo();
    let scope = root.path().join(".agents/skills");
    skill(
        &scope,
        "outer",
        "---\nname: outer\ndescription: A skill with things around it.\n---\n",
    );
    // A directory holding skills rather than being one, and a skill nested
    // inside a skill: neither is discovered.
    skill(
        &scope.join("outer"),
        "inner",
        "---\nname: inner\ndescription: Too deep to be found.\n---\n",
    );
    std::fs::create_dir_all(scope.join("not-a-skill/reference")).expect("a plain directory");

    let catalog = scanned(&workspace, None);

    let names: Vec<&str> = catalog
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(names, vec!["outer"], "{catalog:?}");
    assert!(catalog.refused.is_empty(), "{catalog:?}");
}

#[test]
fn the_repo_scope_never_reaches_above_the_repository_root() {
    let above = TempDir::new().expect("a directory above the repository");
    let root = above.path().join("repo");
    std::fs::create_dir_all(&root).expect("a repository");
    crate::fixtures::init(&root);
    let workspace = root.join("a/b/c");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    skill(
        &above.path().join(".agents/skills"),
        "not-ours",
        "---\nname: not-ours\ndescription: Somebody else's repository.\n---\n",
    );

    let catalog = scanned(&workspace, None);

    assert!(
        catalog.skills.is_empty(),
        "a scan stops at the root it belongs to: {catalog:?}"
    );
}

#[test]
fn every_skills_directory_from_the_workspace_up_to_the_root_is_read_and_the_nearest_wins() {
    let (root, _) = repo();
    let workspace = root.path().join("crates/core/src");
    skill(
        &root.path().join(".agents/skills"),
        "root-skill",
        "---\nname: root-skill\ndescription: The repository's own.\n---\n",
    );
    // A name both the crate and the root carry, so the nearer scope is held to
    // taking it rather than being read beside the farther one.
    skill(
        &root.path().join(".agents/skills"),
        "shared",
        "---\nname: shared\ndescription: The repository's.\n---\n",
    );
    skill(
        &workspace.join(".agents/skills"),
        "nearest-skill",
        "---\nname: nearest-skill\ndescription: Belongs to this crate.\n---\n",
    );
    skill(
        &workspace.join(".agents/skills"),
        "shared",
        "---\nname: shared\ndescription: This crate's.\n---\n",
    );

    let catalog = scanned(&workspace, None);

    let names: Vec<&str> = catalog
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["nearest-skill", "root-skill", "shared"],
        "a crate's own skills do not hide the root's: {catalog:?}"
    );
    let shared = catalog
        .skills
        .iter()
        .find(|skill| skill.name == "shared")
        .expect("the name is held");
    assert_eq!(
        shared.description, "This crate's.",
        "the nearer a scope is to the workspace, the more it says"
    );
    assert_eq!(catalog.shadowed.len(), 1, "the root's one is recorded");
    assert_eq!(
        catalog.shadowed[0].winner,
        Origin {
            scope: Scope::Repo,
            name: "shared".to_string(),
            dir: Some(workspace.join(".agents/skills/shared")),
        }
    );
}

#[test]
fn a_workspace_outside_any_repository_has_no_repo_scope() {
    let (root, workspace) = repo();
    skill(
        &root.path().join(".agents/skills"),
        "root-skill",
        "---\nname: root-skill\ndescription: The repository's own.\n---\n",
    );
    // The same tree, read from a directory that is not inside a repository at
    // all: nothing above it is its scope.
    let loose = TempDir::new().expect("a directory with no repository");

    assert!(scanned(loose.path(), None).skills.is_empty());
    assert_eq!(scanned(&workspace, None).skills.len(), 1);
}
