//! The frontmatter a real collection carries — quoted descriptions with
//! colons, nested metadata, keys no specification knows — and the two ways a
//! `SKILL.md` is refused: no description, and no YAML to read.

use crate::fixtures::{repo, scanned, skill};

#[test]
fn the_two_invocation_flags_are_read_as_top_level_flags() {
    let (root, workspace) = repo();
    let dir = root.path().join(".agents/skills");
    skill(
        &dir,
        "user-only",
        "---\nname: user-only\ndescription: Reached for by a human.\ndisable-model-invocation: true\n---\n",
    );
    skill(
        &dir,
        "model-only",
        "---\nname: model-only\ndescription: Reached for by the model.\nuser-invocable: false\n---\n",
    );

    let catalog = scanned(&workspace, None);

    let model_only = &catalog.skills[0];
    assert_eq!(model_only.name, "model-only", "sorted by name");
    assert!(
        !model_only.user_invocable,
        "`user-invocable: false` is read"
    );
    assert!(model_only.model_invocable);
    let user_only = &catalog.skills[1];
    assert_eq!(user_only.name, "user-only");
    assert!(
        !user_only.model_invocable,
        "`disable-model-invocation: true` is read"
    );
    assert!(user_only.user_invocable);
}

#[test]
fn the_frontmatter_shapes_a_real_collection_carries_are_read_and_unknown_keys_ignored() {
    let (root, workspace) = repo();
    let dir = root.path().join(".agents/skills");
    // A quoted description holding a colon and an escaped quote, copied from a
    // collection in the wild.
    skill(
        &dir,
        "code-review",
        "---\nname: code-review\ndescription: \"Review along two axes: standards, and spec. Use when the user asks to \\\"review since X\\\".\"\n---\n",
    );
    // A two-level nested metadata map, which no schema here reads.
    skill(
        &dir,
        "pr",
        "---\nname: pr\ndescription: Use when writing a PR body.\nmetadata:\n  credits:\n    skill: show-me\n    author: Dex Horthy\n    url: \"https://example.test/skill\"\n---\n",
    );
    // Keys in no version of the specification, beside the two that matter.
    skill(
        &dir,
        "teach",
        "---\nname: teach\ndescription: Teach a concept within the workspace.\nargument-hint: \"What would you like to learn about?\"\nlicense: MIT\nallowed-tools: bash rg\n---\n",
    );

    let catalog = scanned(&workspace, None);

    assert!(
        catalog.refused.is_empty(),
        "nothing here is worth refusing: {catalog:?}"
    );
    assert_eq!(catalog.skills.len(), 3, "{catalog:?}");
    assert_eq!(
        catalog.skills[0].description,
        "Review along two axes: standards, and spec. Use when the user asks to \"review since X\"."
    );
    assert_eq!(catalog.skills[1].description, "Use when writing a PR body.");
    assert_eq!(
        catalog.skills[2].description,
        "Teach a concept within the workspace."
    );
}

#[test]
fn a_skill_with_no_description_is_refused_with_its_file_and_line() {
    let (root, workspace) = repo();
    let dir = root.path().join(".agents/skills");
    skill(
        &dir,
        "nameless-description",
        "---\nname: nameless-description\nmetadata:\n  note: a map is not a description\n---\n",
    );
    skill(
        &dir,
        "empty-description",
        "---\nname: empty-description\ndescription:\n---\n",
    );

    let catalog = scanned(&workspace, None);

    assert!(catalog.skills.is_empty(), "neither is a skill: {catalog:?}");
    assert_eq!(catalog.refused.len(), 2, "{catalog:?}");
    assert_eq!(
        catalog.refused[0].path,
        dir.join("empty-description/SKILL.md"),
        "the file is named"
    );
    assert_eq!(
        catalog.refused[0].line,
        Some(3),
        "the line that broke it is the key's own"
    );
    assert_eq!(
        catalog.refused[1].path,
        dir.join("nameless-description/SKILL.md")
    );
    assert_eq!(
        catalog.refused[1].line,
        Some(1),
        "a key that is absent is blamed on the fence the file opens with"
    );
    assert!(
        catalog.refused[1].reason.contains("description"),
        "{catalog:?}"
    );
}

#[test]
fn a_skill_md_that_is_not_yaml_is_refused_with_the_line_that_broke_it() {
    let (root, workspace) = repo();
    let dir = root.path().join(".agents/skills");
    skill(
        &dir,
        "broken",
        "---\nname: broken\ndescription: A description\n  that opens a block it never closes: [\n---\n",
    );
    skill(
        &dir,
        "unfenced",
        "name: unfenced\ndescription: No fence at all.\n",
    );

    let catalog = scanned(&workspace, None);

    assert!(catalog.skills.is_empty(), "{catalog:?}");
    assert_eq!(catalog.refused.len(), 2, "{catalog:?}");
    assert_eq!(catalog.refused[0].path, dir.join("broken/SKILL.md"));
    assert_eq!(
        catalog.refused[0].line,
        Some(4),
        "the line the scanner choked on: {catalog:?}"
    );
    assert!(
        catalog.refused[0].reason.contains("not YAML"),
        "{catalog:?}"
    );
    assert_eq!(catalog.refused[1].path, dir.join("unfenced/SKILL.md"));
    assert_eq!(catalog.refused[1].line, Some(1));
}
