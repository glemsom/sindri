//! Proves what a run registers and what it says about each tool: the skill tool
//! names every skill in force with the description discovery found for it, the
//! catalog inside it is the one `doctor` prints, and a run with no skill registers
//! `bash` alone rather than a skill tool that refuses.

use std::path::{Path, PathBuf};

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::skills::{Catalog, Listing};
use sindri_core::tools::{BASH, SKILL, ToolSet};
use tempfile::TempDir;

/// A tree of the shape discovery reads: a repository root holding `.git`, a
/// workspace nested inside it, and a home beside it.
struct Tree {
    root: TempDir,
}

impl Tree {
    /// A tree with the three directories a fixture needs and nothing else.
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

    /// `~` for this fixture.
    fn home(&self) -> PathBuf {
        self.at("home")
    }

    /// The repository root: the directory holding `.git`.
    fn repo(&self) -> PathBuf {
        self.at("repo")
    }

    /// The directory a run stands in, two levels below the repository root.
    fn workspace(&self) -> PathBuf {
        self.at("repo/work/deep")
    }

    /// A skill named `name`, described as `description`, under the `.agents`
    /// directory of `scope_dir`.
    fn skill(&self, scope_dir: &Path, name: &str, description: &str) {
        let directory = scope_dir.join(".agents/skills").join(name);
        std::fs::create_dir_all(&directory).expect("the skill directory");
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\nBody of {name}.\n"),
        )
        .expect("the skill file");
    }

    /// What the binary's own build offers here: the disk, and no built-in, so
    /// every skill a test sees is one it wrote.
    fn catalog(&self) -> Catalog {
        Catalog::discover_with_builtin(&self.workspace(), Some(&self.home()), &[])
    }
}

/// The boundary a run in `workspace` holds, from a config that names no
/// exception.
fn boundary(workspace: &Path) -> Boundary {
    Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
        .expect("the boundary holds")
}

/// The catalog entries a tool description carries, in the order it writes them:
/// the shape a model reads the catalog in.
fn entries(description: &str) -> Vec<(String, String)> {
    description
        .lines()
        .filter_map(|line| line.strip_prefix("- `"))
        .map(|entry| {
            let (name, described) = entry
                .split_once("` — ")
                .expect("an entry is a name and the description beside it");
            (name.to_owned(), described.to_owned())
        })
        .collect()
}

/// The listing as the pairs a description carries, so the two renderings of one
/// catalog compare as data rather than as prose.
fn pairs(listing: &[Listing]) -> Vec<(String, String)> {
    listing
        .iter()
        .map(|entry| (entry.name.clone(), entry.description.clone()))
        .collect()
}

#[test]
fn a_workspace_with_skills_lists_them_and_their_descriptions_in_the_tool_description() {
    let tree = Tree::new();
    tree.skill(&tree.repo(), "deploy", "ship the branch");
    tree.skill(&tree.home(), "pdf", "fill in a form");

    let tools = ToolSet::new(&boundary(&tree.workspace()), &tree.catalog());

    let skill = tools
        .get(SKILL)
        .expect("a workspace with skills registers the skill tool");
    assert_eq!(
        entries(skill.description()),
        vec![
            (String::from("deploy"), String::from("ship the branch")),
            (String::from("pdf"), String::from("fill in a form")),
        ],
        "the description lists each skill by name with the line discovery found for it"
    );
}

#[test]
fn the_tool_description_and_the_doctor_listing_read_the_same_catalog() {
    let tree = Tree::new();
    tree.skill(&tree.repo(), "deploy", "ship the branch");
    tree.skill(&tree.home(), "pdf", "fill in a form");

    let catalog = tree.catalog();
    let description = catalog
        .tool_description()
        .expect("a catalog with skills has a description");

    assert_eq!(
        entries(&description),
        pairs(&catalog.listing()),
        "the words the model reads and the lines `doctor` prints are one value, two renderings"
    );
}

#[test]
fn a_catalog_with_no_skill_registers_bash_and_no_skill_tool() {
    let tree = Tree::new();

    let tools = ToolSet::new(&boundary(&tree.workspace()), &tree.catalog());

    assert!(
        tools.get(SKILL).is_none(),
        "no skill means no skill tool at all, not a tool that says it has nothing"
    );
    assert_eq!(
        tools
            .tools()
            .iter()
            .map(|tool| tool.name())
            .collect::<Vec<_>>(),
        vec![BASH],
        "the shell is the one tool a run cannot do without"
    );
}

#[test]
fn the_bash_tool_carries_the_description_the_boundary_renders() {
    let tree = Tree::new();
    let boundary = boundary(&tree.workspace());

    let tools = ToolSet::new(&boundary, &tree.catalog());

    assert_eq!(
        tools
            .get(BASH)
            .expect("bash is always registered")
            .description(),
        boundary.description(),
        "the description is derived from the boundary in force, never written beside it"
    );
}
