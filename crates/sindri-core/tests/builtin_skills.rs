use std::path::PathBuf;
use std::process::Command;

use sindri_core::skills::{Catalog, Scope, Shadowing};
use tempfile::TempDir;

fn empty_world() -> (TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("a temp dir");
    let workspace = root.path().join("workspace");
    let home = root.path().join("home");
    std::fs::create_dir_all(&workspace).expect("the workspace");
    std::fs::create_dir_all(&home).expect("the home");
    (root, workspace, home)
}

#[test]
fn builtin_skills_ship_compiled_in_and_parse_as_discovery_reads_them() {
    let (_root, workspace, home) = empty_world();

    let catalog = Catalog::discover(&workspace, Some(&home));

    assert!(
        catalog.diagnostics().is_empty(),
        "a compiled-in SKILL.md that will not parse must fail the build, not the user: {:?}",
        catalog.diagnostics()
    );
    assert!(
        !catalog.skills().is_empty(),
        "the binary carries at least one skill"
    );
    for skill in catalog.skills() {
        assert_eq!(
            skill.scope,
            Scope::Builtin,
            "{} can only have come from the binary here",
            skill.name
        );
        assert_eq!(
            skill.directory, None,
            "{} has no path, which is the point of compiling it in",
            skill.name
        );
        assert!(
            !skill.description.trim().is_empty(),
            "{} shows a description in the catalog",
            skill.name
        );
        assert!(
            skill.body.starts_with("# "),
            "{} hands back instructions with the frontmatter already stripped",
            skill.name
        );
    }
}

#[test]
fn commit_message_is_not_builtin_but_can_be_supplied_by_the_repository() {
    let (_root, workspace, home) = empty_world();
    assert!(
        Catalog::discover(&workspace, Some(&home))
            .get("commit-message")
            .is_none()
    );

    let directory = workspace.join(".agents/skills/commit-message");
    std::fs::create_dir_all(&directory).expect("the skill directory");
    std::fs::write(
        directory.join("SKILL.md"),
        "---\nname: commit-message\ndescription: the repo's way\n---\n\nThe repo's body.\n",
    )
    .expect("the SKILL.md");

    let catalog = Catalog::discover(&workspace, Some(&home));
    let skill = catalog
        .get("commit-message")
        .expect("the repository's skill loads");
    assert_eq!(skill.scope, Scope::Repo);
    let row = catalog
        .listing()
        .into_iter()
        .find(|row| row.name == "commit-message")
        .expect("the skill is listed");
    assert!(row.shadows.is_empty());
}

#[test]
fn web_access_is_embedded_and_invocable_by_model_and_human() {
    let (_root, workspace, home) = empty_world();
    let catalog = Catalog::discover(&workspace, Some(&home));

    let skill = catalog
        .get("web-access")
        .expect("web-access ships built in");
    assert_eq!(skill.scope, Scope::Builtin);
    assert_eq!(skill.directory, None);
    assert!(skill.model_invocable());
    assert!(skill.human_invocable());
    assert!(skill.body.starts_with("# Web access"));
}

#[test]
fn the_listing_a_doctor_would_print_names_the_builtin_scope() {
    let (_root, workspace, home) = empty_world();

    let listing = Catalog::discover(&workspace, Some(&home)).listing();

    let row = listing
        .iter()
        .find(|row| row.name == "web-access")
        .expect("the built-in web-access skill is listed");
    assert_eq!(row.scope, Scope::Builtin);
    assert_eq!(row.scope.word(), "builtin", "the word a doctor prints");
    assert!(
        !row.description.trim().is_empty(),
        "the listing shows what the skill is for"
    );
    assert!(
        row.shadows.is_empty(),
        "nothing on disk took the name from it"
    );
}

#[test]
fn a_repo_skill_takes_the_name_and_the_listing_names_the_builtin_it_shadowed() {
    let (_root, workspace, home) = empty_world();
    let directory = workspace.join(".agents/skills/web-access");
    std::fs::create_dir_all(&directory).expect("the skill directory");
    std::fs::write(
        directory.join("SKILL.md"),
        "---\nname: web-access\ndescription: the repo's way\n---\n\nThe repo's body.\n",
    )
    .expect("the SKILL.md");

    let listing = Catalog::discover(&workspace, Some(&home)).listing();

    let row = listing
        .iter()
        .find(|row| row.name == "web-access")
        .expect("the name is still listed once");
    assert_eq!(
        row.scope,
        Scope::Repo,
        "the repo's skill is the one in force"
    );
    assert_eq!(
        row.shadows,
        vec![Shadowing {
            name: String::from("web-access"),
            winner: Scope::Repo,
            winner_path: Some(directory),
            loser: Scope::Builtin,
            loser_path: None,
        }],
        "the built-in it displaced is named beside it, not dropped"
    );
}

const SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/skills");

const PROOF_CHILD: &str = "SINDRI_BUILTIN_PROOF_CHILD";

const PROOF_TEST: &str = "removing_the_source_directory_leaves_the_builtin_in_a_fresh_process";

struct Restore {
    from: PathBuf,
    to: PathBuf,
}

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::rename(&self.from, &self.to);
    }
}

#[test]
fn removing_the_source_directory_leaves_the_builtin_in_a_fresh_process() {
    if std::env::var_os(PROOF_CHILD).is_some() {
        prove_the_builtin_survives();
    }

    let source = PathBuf::from(SOURCE);
    assert!(
        source.is_dir(),
        "the source directory {} exists in a checkout; if this test was killed, \
         rename it back by hand",
        source.display()
    );
    let aside = source.with_file_name(format!("skills-away-{}", std::process::id()));
    std::fs::rename(&source, &aside).expect("the source directory moves aside");
    let restore = Restore {
        from: aside,
        to: source.clone(),
    };

    let child = Command::new(std::env::current_exe().expect("the test binary's own path"))
        .args(["--exact", PROOF_TEST, "--nocapture"])
        .env(PROOF_CHILD, "1")
        .output()
        .expect("the test binary runs a second time, as a child");

    assert!(
        !source.exists(),
        "the directory stayed gone for the whole of the child's run"
    );
    assert!(
        child.status.success(),
        "a process started with the source directory gone still finds the built-in:\n\
         status: {}\nstdout:\n{}\nstderr:\n{}",
        child.status,
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(
        String::from_utf8_lossy(&child.stdout).contains("web-access\tbuiltin"),
        "web-access survives without its source directory too"
    );

    drop(restore);
    assert!(source.is_dir(), "the source directory is back where it was");
}

fn prove_the_builtin_survives() -> ! {
    let (_root, workspace, home) = empty_world();
    let catalog = Catalog::discover(&workspace, Some(&home));

    for row in catalog.listing() {
        println!("{}\t{}", row.name, row.scope.word());
    }
    for diagnostic in catalog.diagnostics() {
        eprintln!(
            "{}:{}: {}",
            diagnostic.path.display(),
            diagnostic.line,
            diagnostic.message
        );
    }

    let intact = catalog.diagnostics().is_empty()
        && !catalog.skills().is_empty()
        && catalog
            .skills()
            .iter()
            .all(|skill| skill.scope == Scope::Builtin && !skill.body.trim().is_empty());
    std::process::exit(if intact { 0 } else { 1 });
}

#[test]
fn subagents_is_discoverable_and_loadable() {
    let (_root, workspace, home) = empty_world();
    let catalog = Catalog::discover(&workspace, Some(&home));
    let skill = catalog
        .skills()
        .iter()
        .find(|skill| skill.name == "subagents")
        .expect("builtin subagents");
    assert_eq!(skill.scope, Scope::Builtin);
    assert!(skill.model_invocable());
    assert!(skill.wrapper().contains("Bash runner"));
}

#[test]
fn subagents_obeys_user_then_repository_shadowing() {
    let (_root, workspace, home) = empty_world();
    for (base, scope) in [(&home, Scope::User), (&workspace, Scope::Repo)] {
        let directory = base.join(".agents/skills/subagents");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            "---\nname: subagents\ndescription: local delegation\n---\n# Local delegation\n",
        )
        .unwrap();
        let catalog = Catalog::discover(&workspace, Some(&home));
        let skill = catalog
            .skills()
            .iter()
            .find(|skill| skill.name == "subagents")
            .unwrap();
        assert_eq!(skill.scope, scope);
        assert!(skill.wrapper().contains("Local delegation"));
    }
}
