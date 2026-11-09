//! Proves what a run registers and what it says about each tool: the skill tool names every skill
//! in force with the description discovery found, the catalog inside it is the one `doctor` prints,
//! and a run with no skill registers `bash` alone rather than a skill tool that refuses.

use std::path::{Path, PathBuf};

use sindri_core::config::Config;
use sindri_core::open::{EXTENSIONS, SCHEMES};
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::skills::{Catalog, Listing};
use sindri_core::tools::{BASH, FrontEnd, OPEN, SKILL, ToolSet};
use tempfile::TempDir;

struct Tree {
    root: TempDir,
}

impl Tree {
    fn new() -> Tree {
        let root = tempfile::tempdir().expect("a temp dir");
        let tree = Tree { root };
        std::fs::create_dir_all(tree.repo().join(".git")).expect("the git marker");
        std::fs::create_dir_all(tree.workspace()).expect("the workspace");
        std::fs::create_dir_all(tree.home()).expect("the home");
        tree
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn home(&self) -> PathBuf {
        self.at("home")
    }

    fn repo(&self) -> PathBuf {
        self.at("repo")
    }

    fn workspace(&self) -> PathBuf {
        self.at("repo/work/deep")
    }

    fn skill(&self, scope_dir: &Path, name: &str, description: &str) {
        let directory = scope_dir.join(".agents/skills").join(name);
        std::fs::create_dir_all(&directory).expect("the skill directory");
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\nBody of {name}.\n"),
        )
        .expect("the skill file");
    }

    fn flagged(&self, scope_dir: &Path, name: &str, description: &str, flag: &str) {
        let directory = scope_dir.join(".agents/skills").join(name);
        std::fs::create_dir_all(&directory).expect("the skill directory");
        std::fs::write(
            directory.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: {description}\n{flag}\n---\n\nBody of {name}.\n"
            ),
        )
        .expect("the skill file");
    }

    fn catalog(&self) -> Catalog {
        Catalog::discover_with_builtin(&self.workspace(), Some(&self.home()), &[])
    }
}

fn boundary(workspace: &Path) -> Boundary {
    Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
        .expect("the boundary holds")
}

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

    let tools = ToolSet::new(
        &boundary(&tree.workspace()),
        &tree.catalog(),
        FrontEnd::Terminal,
    );

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

    let tools = ToolSet::new(
        &boundary(&tree.workspace()),
        &tree.catalog(),
        FrontEnd::Terminal,
    );

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
        vec![BASH, OPEN],
        "a terminal registers the shell and the browser, and no skill tool at all"
    );
}

#[test]
fn the_bash_tool_carries_the_description_the_boundary_renders() {
    let tree = Tree::new();
    let boundary = boundary(&tree.workspace());

    let tools = ToolSet::new(&boundary, &tree.catalog(), FrontEnd::Terminal);

    assert_eq!(
        tools
            .get(BASH)
            .expect("bash is always registered")
            .description(),
        boundary.description(),
        "the description is derived from the boundary in force, never written beside it"
    );
}

fn words(description: &str) -> Vec<String> {
    description
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[test]
fn a_batch_tool_set_has_no_open_and_its_schema_never_mentions_it() {
    let tree = Tree::new();

    let tools = ToolSet::new(
        &boundary(&tree.workspace()),
        &tree.catalog(),
        FrontEnd::Batch,
    );

    assert!(
        tools.get(OPEN).is_none(),
        "a batch run has nobody to look, so the tool is absent rather than refused"
    );
    for tool in tools.tools() {
        assert_ne!(
            tool.name(),
            OPEN,
            "no tool of that name is registered at all"
        );
        assert!(
            !words(tool.description()).contains(&String::from("open")),
            "`{}` never says the word, so a turn is never spent finding out: {}",
            tool.name(),
            tool.description()
        );
    }
}

#[test]
fn a_terminal_tool_set_registers_open_after_bash_and_the_skill_tool() {
    let tree = Tree::new();
    tree.skill(&tree.repo(), "deploy", "ship the branch");

    let tools = ToolSet::new(
        &boundary(&tree.workspace()),
        &tree.catalog(),
        FrontEnd::Terminal,
    );

    assert_eq!(
        tools
            .tools()
            .iter()
            .map(|tool| tool.name())
            .collect::<Vec<_>>(),
        vec![BASH, SKILL, OPEN],
        "three is the ceiling, and the order is the order a front end registers"
    );
}

#[test]
fn the_open_tool_names_every_extension_and_every_scheme_it_accepts() {
    let tree = Tree::new();
    let boundary = boundary(&tree.workspace());

    let tools = ToolSet::new(&boundary, &tree.catalog(), FrontEnd::Terminal);

    let description = tools
        .get(OPEN)
        .expect("a terminal registers the open tool")
        .description();
    for extension in EXTENSIONS {
        assert!(
            description.contains(extension),
            "the model reads `{extension}` in the description, so a refusal is a reminder and not \
             a surprise: {description}"
        );
    }
    for scheme in SCHEMES {
        assert!(
            description.contains(scheme),
            "and `{scheme}`, for the same reason: {description}"
        );
    }
    assert!(
        description.contains(&boundary.writable().paths()[0].display().to_string()),
        "the description is derived from the boundary in force, as `bash`'s is: {description}"
    );
}

#[test]
fn a_user_only_skill_is_out_of_the_tool_schema_and_still_in_the_doctor_listing() {
    let tree = Tree::new();
    tree.skill(&tree.repo(), "deploy", "ship the branch");
    tree.flagged(
        &tree.repo(),
        "handover",
        "the person's own notes",
        "disable-model-invocation: true",
    );
    let catalog = tree.catalog();

    let skill = ToolSet::new(&boundary(&tree.workspace()), &catalog, FrontEnd::Terminal)
        .get(SKILL)
        .expect("one skill is still the model's to call")
        .description()
        .to_owned();

    assert_eq!(
        entries(&skill),
        vec![(String::from("deploy"), String::from("ship the branch"))],
        "the schema names only the skills the model may call"
    );
    assert!(
        !skill.contains("handover"),
        "a user-only skill is absent from the schema rather than listed and refused: {skill}"
    );
    assert!(
        catalog.get("handover").is_some()
            && catalog.listing().iter().any(|row| row.name == "handover"),
        "the catalog still holds it, so `doctor` can list it and the person can ask for it"
    );
}

#[test]
fn a_catalog_of_only_user_only_skills_registers_no_skill_tool() {
    let tree = Tree::new();
    tree.flagged(
        &tree.repo(),
        "handover",
        "the person's own notes",
        "disable-model-invocation: true",
    );

    let tools = ToolSet::new(
        &boundary(&tree.workspace()),
        &tree.catalog(),
        FrontEnd::Terminal,
    );

    assert_eq!(
        tools
            .tools()
            .iter()
            .map(|tool| tool.name())
            .collect::<Vec<_>>(),
        vec![BASH, OPEN],
        "nothing the model may call means no skill tool, not one that names nothing"
    );
}

#[test]
fn a_model_only_skill_still_registers_the_skill_tool() {
    let tree = Tree::new();
    tree.flagged(
        &tree.repo(),
        "sweeper",
        "tidy the ledger",
        "user-invocable: false",
    );

    let catalog = tree.catalog();
    let tools = ToolSet::new(&boundary(&tree.workspace()), &catalog, FrontEnd::Terminal);
    let skill = tools
        .get(SKILL)
        .expect("a skill only the model may call still needs the tool that calls it")
        .description();

    assert_eq!(
        entries(skill),
        vec![(String::from("sweeper"), String::from("tidy the ledger"))],
        "the only door this skill is behind is the model's, so the schema names it"
    );
}
