//! Proves the preamble is a function of the workspace and of nothing else: a
//! build whose catalog offers skills, and one whose catalog offers none, must
//! render the very same bytes, while the tool sets either side of it differ.
//!
//! Every fixture is a temp tree with its own `.git` marker, so the repository
//! root a preamble reads `AGENTS.md` from is never the tree these tests live in.

use std::path::{Path, PathBuf};

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::preamble::Preamble;
use sindri_core::skills::Catalog;
use sindri_core::tools::ToolSet;
use tempfile::TempDir;

/// A tree a preamble, a catalog and a boundary are all read from: a repository
/// root holding a `.git` marker, a workspace inside it, and a home beside it.
struct Tree {
    root: TempDir,
}

impl Tree {
    /// A tree with nothing in it beyond the three directories a fixture needs.
    fn new() -> Tree {
        let root = tempfile::tempdir().expect("a temp dir");
        let tree = Tree { root };
        std::fs::create_dir_all(tree.repo().join(".git")).expect("the git marker");
        std::fs::create_dir_all(tree.workspace()).expect("the workspace");
        std::fs::create_dir_all(tree.home()).expect("the home");
        tree
    }

    /// An `AGENTS.md` holding `text`, written into `directory`.
    fn agents(&self, directory: &Path, text: &str) {
        std::fs::create_dir_all(directory).expect("the directory");
        std::fs::write(directory.join("AGENTS.md"), text).expect("the instructions");
    }

    /// A path inside the tree, whether or not anything is there yet.
    fn at(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    /// `~` for this fixture.
    fn home(&self) -> PathBuf {
        self.at("home")
    }

    /// The repository root: the directory holding `.git`.
    fn repo(&self) -> PathBuf {
        self.at("repo")
    }

    /// The directory a run stands in.
    fn workspace(&self) -> PathBuf {
        self.at("repo/work/deep")
    }
}

/// The boundary a run in `workspace` holds, from a config that names no
/// exception.
fn boundary(workspace: &Path) -> Boundary {
    Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
        .expect("the boundary holds")
}

/// The two things a front end sends for a workspace, assembled through the same
/// public seams a run assembles them through: the system message, and the tool
/// set the model's schema will hold.
fn assembled(workspace: &Path, catalog: &Catalog) -> (String, ToolSet) {
    (
        Preamble::for_workspace(workspace).render(),
        ToolSet::new(&boundary(workspace), catalog),
    )
}

#[test]
fn a_catalog_offering_no_skill_leaves_the_preamble_byte_identical() {
    let tree = Tree::new();

    // One workspace, two builds of it: the real one, whose built-ins are compiled
    // in, and a build carrying no skill support at all. Nothing on disk differs
    // between them, so nothing but the tool set can.
    let offering = Catalog::discover(&tree.workspace(), Some(&tree.home()));
    let no_skill_support =
        Catalog::discover_with_builtin(&tree.workspace(), Some(&tree.home()), &[]);
    assert!(
        !offering.is_empty(),
        "the real build offers its compiled-in skills"
    );
    assert!(
        no_skill_support.is_empty(),
        "a build with no skill support and a workspace full of none offers nothing"
    );

    let (offering_preamble, offering_tools) = assembled(&tree.workspace(), &offering);
    let (bare_preamble, bare_tools) = assembled(&tree.workspace(), &no_skill_support);

    assert!(
        offering_tools.get("skill").is_some(),
        "a catalog with a skill registers the skill tool"
    );
    assert!(
        bare_tools.get("skill").is_none(),
        "no model-visible skill means no skill tool at all, not a tool that refuses"
    );
    assert_eq!(
        offering_preamble.as_bytes(),
        bare_preamble.as_bytes(),
        "skill support must cost the preamble not one byte"
    );
    for skill in offering.skills() {
        assert!(
            !offering_preamble.contains(&skill.name),
            "the catalog belongs in the tool description: {} must not reach the preamble",
            skill.name
        );
    }
}

#[test]
fn the_preamble_says_who_answers_what_shapes_it_and_where_it_stands() {
    let tree = Tree::new();

    let preamble = Preamble::for_workspace(&tree.workspace()).render();

    let persona = preamble.find("## Persona").expect("a persona section");
    let principles = preamble
        .find("## Principles")
        .expect("a principles section");
    let environment = preamble
        .find("## Environment")
        .expect("an environment section");
    assert!(
        persona < principles && principles < environment,
        "the sections arrive in the order §9 names them"
    );
    assert!(
        preamble.contains("GNU/Linux"),
        "the environment names the platform the shell is a first choice on"
    );
    assert!(
        preamble.contains("pipefail") && preamble.contains("$TMPDIR"),
        "a staged command and the scratch a run makes are both named"
    );
    assert!(
        preamble.contains(&tree.workspace().display().to_string()),
        "the one directory the run stands in is named, because relative paths resolve there"
    );
}

#[test]
fn a_repository_root_agents_md_arrives_as_project_instructions() {
    let tree = Tree::new();
    tree.agents(
        &tree.repo(),
        "# House rules\n\nRun the checks before you commit.\n",
    );

    let preamble = Preamble::for_workspace(&tree.workspace()).render();

    let (_, instructions) = preamble
        .split_once("<project_instructions>\n")
        .expect("the project's own words are fenced");
    assert_eq!(
        instructions, "# House rules\n\nRun the checks before you commit.\n</project_instructions>",
        "the project's words arrive last, in their own fence, with nothing after them"
    );
}

#[test]
fn a_nested_agents_md_is_never_read() {
    let tree = Tree::new();
    tree.agents(
        &tree.repo().join("work"),
        "nested rules that are not the project's",
    );
    tree.agents(
        &tree.workspace(),
        "the workspace's own rules, which are not the project's",
    );

    let preamble = Preamble::for_workspace(&tree.workspace()).render();

    assert!(
        !preamble.contains("<project_instructions>"),
        "only the repository root's AGENTS.md is read, never a nested one"
    );
}

#[test]
fn a_workspace_with_no_agents_md_adds_no_project_instructions() {
    let tree = Tree::new();

    let preamble = Preamble::for_workspace(&tree.workspace()).render();

    assert!(
        !preamble.contains("<project_instructions>"),
        "absent means nothing is added, not an empty section"
    );
}
