//! Proves skill discovery across the three scopes: what a workspace, a home and
//! the compiled-in registry offer, which of two skills of one name survives, and
//! that a scope the repository root does not own is never read at all.
//!
//! Every fixture is built under a temp dir with its own `.git` marker and its own
//! home, so discovery is never pointed at the tree these tests live in and the
//! person's real `~/.agents/skills` never decides a test.

use std::path::{Path, PathBuf};

use sindri_core::skills::{Catalog, Diagnostic, Scope, Shadowing};
use tempfile::TempDir;

/// A tree holding everything discovery reads: a home beside a repository root,
/// a workspace nested inside that root, and no ancestor it should ever visit.
struct Tree {
    root: TempDir,
}

impl Tree {
    /// A tree with a repository root (`repo/`, holding `.git`) and a workspace
    /// three directories down inside it.
    fn new() -> Tree {
        let root = tempfile::tempdir().expect("a temp dir");
        let tree = Tree { root };
        std::fs::create_dir_all(tree.repo().join(".git")).expect("the git marker");
        std::fs::create_dir_all(tree.workspace()).expect("the workspace");
        std::fs::create_dir_all(tree.home()).expect("the home");
        tree
    }

    /// A path inside the tree, whether or not anything is there yet.
    fn at(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    /// The home discovery is told about: `~` for this fixture.
    fn home(&self) -> PathBuf {
        self.at("home")
    }

    /// The repository root: the directory holding `.git`.
    fn repo(&self) -> PathBuf {
        self.at("repo")
    }

    /// The workspace: `repo/work/deep`, so two `.agents/skills` sit between it
    /// and the repository root.
    fn workspace(&self) -> PathBuf {
        self.at("repo/work/deep")
    }

    /// A skill directory named `name` under `scope_dir`, holding a `SKILL.md`
    /// whose frontmatter carries `name` and `description`.
    fn skill(&self, scope_dir: &Path, name: &str, description: &str) -> PathBuf {
        let directory = scope_dir.join(name);
        write(&directory.join("SKILL.md"), &skill_md(name, description));
        directory
    }
}

/// A symlink at `link` pointing at `target`, with every directory above the link
/// created, so a dangling link is dangling on purpose.
fn symlink(target: &Path, link: &Path) {
    std::fs::create_dir_all(link.parent().expect("a parent")).expect("the directories");
    std::os::unix::fs::symlink(target, link).expect("the symlink");
}

/// Writes `text` to `path`, creating every directory above it.
fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directories");
    std::fs::write(path, text).expect("the file");
}

/// A `SKILL.md` a well-formed skill would hold: frontmatter naming it and
/// describing it, then a body that is the instructions.
fn skill_md(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\nBody of {name}.\n")
}

/// The descriptions in a catalog, by name, so a test can say which skill of a
/// shared name won.
fn described(catalog: &Catalog) -> Vec<(String, String)> {
    catalog
        .skills()
        .iter()
        .map(|skill| (skill.name.clone(), skill.description.clone()))
        .collect()
}

#[test]
fn nothing_found_is_an_empty_catalog_rather_than_no_catalog() {
    let tree = Tree::new();

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert!(catalog.skills().is_empty(), "no scope holds a skill here");
    assert!(catalog.shadowing().is_empty(), "nothing to shadow");
    assert!(
        catalog.diagnostics().is_empty(),
        "nothing to complain about"
    );
    assert!(catalog.is_empty());
}

#[test]
fn a_directory_holding_a_skill_md_is_a_skill_in_its_scope() {
    let tree = Tree::new();
    tree.skill(&tree.home().join(".agents/skills"), "pdf", "fill in a form");
    tree.skill(&tree.repo().join(".agents/skills"), "deploy", "ship it");

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert_eq!(
        described(&catalog),
        vec![
            (String::from("deploy"), String::from("ship it")),
            (String::from("pdf"), String::from("fill in a form")),
        ],
        "both scopes are read, in name order"
    );
    let pdf = catalog.get("pdf").expect("the home's skill");
    assert_eq!(pdf.scope, Scope::User);
    assert_eq!(pdf.body, "Body of pdf.");
    assert_eq!(
        pdf.directory.as_deref(),
        Some(tree.home().join(".agents/skills/pdf").as_path())
    );
    assert_eq!(
        catalog.get("deploy").expect("the repo's skill").scope,
        Scope::Repo
    );
}

#[test]
fn a_nearer_scope_takes_the_name_and_the_loser_is_reported() {
    let tree = Tree::new();
    let home_skill = tree.skill(
        &tree.home().join(".agents/skills"),
        "greet",
        "the home's way",
    );
    let repo_skill = tree.skill(
        &tree.repo().join(".agents/skills"),
        "greet",
        "the repo's way",
    );

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert_eq!(
        described(&catalog),
        vec![(String::from("greet"), String::from("the repo's way"))],
        "one skill of the name, and the repo's"
    );
    assert_eq!(
        catalog.shadowing(),
        &[Shadowing {
            name: String::from("greet"),
            winner: Scope::Repo,
            winner_path: Some(repo_skill),
            loser: Scope::User,
            loser_path: Some(home_skill),
        }],
        "the loser is reported, not silently dropped"
    );
    assert!(catalog.diagnostics().is_empty(), "shadowing is not a fault");
}

#[test]
fn discovery_never_walks_above_the_repository_root() {
    let tree = Tree::new();
    tree.skill(&tree.at(".agents/skills"), "above", "not this repository's");
    tree.skill(
        &tree.repo().join(".agents/skills"),
        "inside",
        "this repository's",
    );

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert_eq!(
        described(&catalog),
        vec![(String::from("inside"), String::from("this repository's"))],
        "the root is read and the directory above it is not"
    );
    assert!(
        catalog.shadowing().is_empty() && catalog.diagnostics().is_empty(),
        "the tempting directory is not even opened, so it leaves no trace"
    );
}

#[test]
fn the_nearest_repo_directory_takes_the_name_from_a_further_one() {
    let tree = Tree::new();
    let far = tree.skill(&tree.repo().join(".agents/skills"), "style", "the root's");
    let near = tree.skill(
        &tree.workspace().join(".agents/skills"),
        "style",
        "the workspace's",
    );
    tree.skill(
        &tree.at("repo/work/.agents/skills"),
        "other",
        "the middle one's",
    );

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert_eq!(
        described(&catalog),
        vec![
            (String::from("other"), String::from("the middle one's")),
            (String::from("style"), String::from("the workspace's")),
        ],
        "every directory between the workspace and the root is read, nearest winning"
    );
    assert_eq!(
        catalog.shadowing(),
        &[Shadowing {
            name: String::from("style"),
            winner: Scope::Repo,
            winner_path: Some(near),
            loser: Scope::Repo,
            loser_path: Some(far),
        }],
        "two directories of one scope still report which one lost"
    );
}

#[test]
fn a_symlinked_skill_directory_is_followed() {
    let tree = Tree::new();
    let shared = tree.at("shared/greet");
    write(
        &shared.join("SKILL.md"),
        &skill_md("greet", "shared from elsewhere"),
    );
    let link = tree.repo().join(".agents/skills/greet");
    symlink(&shared, &link);

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert_eq!(
        described(&catalog),
        vec![(String::from("greet"), String::from("shared from elsewhere"))],
        "a link to a skill directory is a skill directory"
    );
    assert_eq!(
        catalog
            .get("greet")
            .expect("the shared skill")
            .directory
            .as_deref(),
        Some(link.as_path()),
        "it is named where it was found, not where it points"
    );
    assert!(
        catalog.diagnostics().is_empty(),
        "following a link is not a fault"
    );
}

#[test]
fn a_dangling_link_is_refused_by_its_own_path() {
    let tree = Tree::new();
    let scope = tree.repo().join(".agents/skills");
    tree.skill(&scope, "real", "still loads");
    let dangling = scope.join("broken");
    symlink(&tree.at("gone"), &dangling);

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert_eq!(
        described(&catalog),
        vec![(String::from("real"), String::from("still loads"))],
        "the neighbour still loads"
    );
    assert_eq!(
        catalog.diagnostics(),
        &[Diagnostic {
            path: dangling.clone(),
            line: 1,
            message: String::from("this link leads nowhere, so there is no SKILL.md to read"),
        }],
        "the refusal names the link, not the target that is not there"
    );
}

#[test]
fn a_malformed_skill_md_is_a_diagnostic_naming_its_line() {
    let tree = Tree::new();
    let scope = tree.repo().join(".agents/skills");
    tree.skill(&scope, "real", "still loads");
    let broken = scope.join("broken/SKILL.md");
    // A tab may not start a token in YAML, so line 3 is where this cannot parse.
    write(
        &broken,
        "---\nname: broken\n\tdescription: tabbed\n---\n\nBody.\n",
    );

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert_eq!(
        described(&catalog),
        vec![(String::from("real"), String::from("still loads"))],
        "one unparseable skill does not fail the catalog"
    );
    let diagnostic = catalog.diagnostics().first().expect("a diagnostic");
    assert_eq!(diagnostic.path, broken);
    assert_eq!(diagnostic.line, 3, "the line the parser stopped on");
    assert!(
        diagnostic.message.contains("tab"),
        "the parser's own words, in {}",
        diagnostic.message
    );
    assert!(
        !diagnostic.message.contains("line"),
        "the message names no line of its own, because the diagnostic already has one: {}",
        diagnostic.message
    );
}

#[test]
fn a_skill_md_with_no_frontmatter_is_refused_at_its_first_line() {
    let tree = Tree::new();
    let scope = tree.repo().join(".agents/skills");
    tree.skill(&scope, "real", "still loads");
    let bare = scope.join("bare/SKILL.md");
    write(&bare, "Just instructions, and nothing that names them.\n");

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert_eq!(
        described(&catalog),
        vec![(String::from("real"), String::from("still loads"))],
        "the neighbour still loads"
    );
    assert_eq!(
        catalog.diagnostics(),
        &[Diagnostic {
            path: bare,
            line: 1,
            message: String::from("no frontmatter: a SKILL.md opens with a `---` line"),
        }],
        "a skill with no name is refused, not catalogued empty"
    );
}

#[test]
fn a_frontmatter_without_a_description_is_refused_at_the_frontmatter() {
    let tree = Tree::new();
    let scope = tree.repo().join(".agents/skills");
    let nameless = scope.join("nameless/SKILL.md");
    write(&nameless, "---\nname: nameless\n---\n\nBody.\n");

    let catalog = Catalog::discover(&tree.workspace(), Some(&tree.home()));

    assert!(
        catalog.skills().is_empty(),
        "a skill with no description is no catalog entry"
    );
    let diagnostic = catalog.diagnostics().first().expect("a diagnostic");
    assert_eq!(diagnostic.path, nameless);
    assert_eq!(
        diagnostic.line, 2,
        "the frontmatter that should carry the key"
    );
    assert!(
        diagnostic.message.contains("description"),
        "the missing key is named: {}",
        diagnostic.message
    );
}

/// A `SKILL.md` of the kind a build compiles in, so the builtin scope can be
/// proved before any built-in content exists.
const BUILTIN_GREET: &str =
    "---\nname: greet\ndescription: the built-in way\n---\n\nBuilt-in body.\n";

#[test]
fn a_compiled_in_skill_is_a_catalog_entry_with_no_path() {
    let tree = Tree::new();

    let catalog =
        Catalog::discover_with_builtin(&tree.workspace(), Some(&tree.home()), &[BUILTIN_GREET]);

    assert_eq!(
        described(&catalog),
        vec![(String::from("greet"), String::from("the built-in way"))],
        "the builtin scope is a real source, not a placeholder"
    );
    let skill = catalog.get("greet").expect("the built-in skill");
    assert_eq!(skill.scope, Scope::Builtin);
    assert_eq!(skill.directory, None, "a builtin has no path to go stale");
    assert_eq!(skill.body, "Built-in body.");
}

#[test]
fn the_builtin_scope_is_the_lowest_precedence() {
    let tree = Tree::new();
    let repo_skill = tree.skill(
        &tree.repo().join(".agents/skills"),
        "greet",
        "the repo's way",
    );

    let catalog =
        Catalog::discover_with_builtin(&tree.workspace(), Some(&tree.home()), &[BUILTIN_GREET]);

    assert_eq!(
        described(&catalog),
        vec![(String::from("greet"), String::from("the repo's way"))],
        "anything on disk takes a name from the binary"
    );
    assert_eq!(
        catalog.shadowing(),
        &[Shadowing {
            name: String::from("greet"),
            winner: Scope::Repo,
            winner_path: Some(repo_skill),
            loser: Scope::Builtin,
            loser_path: None,
        }],
        "a builtin that loses is reported too, with no path to name"
    );
}
