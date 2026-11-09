//! Proves the built-in skills are compiled into the binary rather than read from a path: they
//! appear in a catalog under the `builtin` scope, they parse under the discovery a run uses, and
//! they survive their source directory being taken away.

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
fn the_listing_a_doctor_would_print_names_the_builtin_scope() {
    let (_root, workspace, home) = empty_world();

    let listing = Catalog::discover(&workspace, Some(&home)).listing();

    let row = listing
        .iter()
        .find(|row| row.name == "commit-message")
        .expect("the built-in commit-message skill is listed");
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
    let directory = workspace.join(".agents/skills/commit-message");
    std::fs::create_dir_all(&directory).expect("the skill directory");
    std::fs::write(
        directory.join("SKILL.md"),
        "---\nname: commit-message\ndescription: the repo's way\n---\n\nThe repo's body.\n",
    )
    .expect("the SKILL.md");

    let listing = Catalog::discover(&workspace, Some(&home)).listing();

    let row = listing
        .iter()
        .find(|row| row.name == "commit-message")
        .expect("the name is still listed once");
    assert_eq!(
        row.scope,
        Scope::Repo,
        "the repo's skill is the one in force"
    );
    assert_eq!(
        row.shadows,
        vec![Shadowing {
            name: String::from("commit-message"),
            winner: Scope::Repo,
            winner_path: Some(directory),
            loser: Scope::Builtin,
            loser_path: None,
        }],
        "the built-in it displaced is named beside it, not dropped"
    );
}

/// The directory the built-ins are written in, as this build saw it: the text is already compiled
/// in, so this is where a person edits a skill and never where a run reads one.
const SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/skills");

const PROOF_CHILD: &str = "SINDRI_BUILTIN_PROOF_CHILD";

const PROOF_TEST: &str = "removing_the_source_directory_leaves_the_builtin_in_a_fresh_process";

/// Puts the source directory back when the test is done with it, whether it ends by returning or by
/// panicking.
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
        String::from_utf8_lossy(&child.stdout).contains("commit-message\tbuiltin"),
        "the child listed the built-in under the builtin scope, and said so: {}",
        String::from_utf8_lossy(&child.stdout)
    );

    drop(restore);
    assert!(source.is_dir(), "the source directory is back where it was");
}

/// The child's whole job: start from nothing, discover, and print what the catalog holds. It exits
/// rather than returning, so the harness never reports on a run whose only purpose is to answer
/// this question.
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
