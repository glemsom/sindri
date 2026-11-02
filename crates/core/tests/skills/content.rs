//! What a loaded skill is made of: the body under the fence with the frontmatter
//! gone, the files beside it, and the two shapes that are a refusal instead.

use crate::fixtures::{repo, scanned, skill};

#[test]
fn the_body_is_everything_under_the_fence_with_the_frontmatter_gone() {
    let (root, workspace) = repo();
    skill(
        &root.path().join(".agents/skills"),
        "style",
        "---\nname: style\ndescription: Short sentences.\n---\n\n# Style\n\nNo adverbs.\n",
    );
    let catalog = scanned(&workspace, None);
    let found = &catalog.skills[0];

    assert_eq!(found.body().as_deref(), Ok("# Style\n\nNo adverbs."));

    let dir = found
        .dir()
        .expect("a scanned skill has a directory")
        .to_path_buf();
    std::fs::create_dir_all(dir.join("references")).expect("a references directory");
    std::fs::write(dir.join("references/api.md"), "# api\n").expect("a reference");
    std::fs::write(dir.join(".hidden"), "not a reference\n").expect("a dot-file");
    assert_eq!(
        found.listing(),
        vec![
            "SKILL.md".to_string(),
            "references/".to_string(),
            "references/api.md".to_string(),
        ],
        "the listing is relative, sorted, and leaves the dot-files out"
    );
}

#[test]
fn a_skill_whose_file_lost_its_body_is_a_refusal_that_names_the_file() {
    let (root, workspace) = repo();
    let dir = root.path().join(".agents/skills");
    skill(
        &dir,
        "hollow",
        "---\nname: hollow\ndescription: Nothing under the fence.\n---",
    );
    // Read before the file changes: the scan is what found it.
    let catalog = scanned(&workspace, None);
    let found = &catalog.skills[0];

    let problem = found.body().expect_err("there is no body to load");
    assert!(
        problem.contains("holds nothing under its frontmatter") && problem.contains("SKILL.md"),
        "the refusal names the file and what is wrong with it: {problem}"
    );
}
