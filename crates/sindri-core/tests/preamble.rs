//! Proves the preamble is a function of the workspace and of nothing else: a build whose catalog
//! offers skills and one whose offers none must render the same bytes, while the tool sets differ.
//! Every fixture is a temp tree with its own `.git`, so the root read is never this tree.

use std::path::{Path, PathBuf};

use sindri_core::config::Config;
use sindri_core::policy::{Boundary, Enforcement};
use sindri_core::preamble::Preamble;
use sindri_core::skills::Catalog;
use sindri_core::tools::{FrontEnd, ToolSet};
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

    fn agents(&self, directory: &Path, text: &str) {
        std::fs::create_dir_all(directory).expect("the directory");
        std::fs::write(directory.join("AGENTS.md"), text).expect("the instructions");
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
}

fn boundary(workspace: &Path) -> Boundary {
    Boundary::from_config(&Config::default(), workspace, Enforcement::Bubblewrap)
        .expect("the boundary holds")
}

fn assembled(workspace: &Path, catalog: &Catalog) -> (String, ToolSet) {
    (
        Preamble::for_workspace(workspace).render(),
        ToolSet::new(&boundary(workspace), catalog, FrontEnd::Terminal),
    )
}

fn principle_bullets(preamble: &str) -> Vec<&str> {
    let (_, after) = preamble
        .split_once("## Principles\n\n")
        .expect("a principles section");
    let section = after
        .split_once("\n## ")
        .map_or(after, |(section, _)| section);
    section
        .lines()
        .filter(|line| line.starts_with("- "))
        .collect()
}

#[test]
fn a_catalog_offering_no_skill_leaves_the_preamble_byte_identical() {
    let tree = Tree::new();

    // One workspace, two builds of it: the real one, whose built-ins are compiled in, and a build
    // carrying no skill support at all. Nothing on disk differs between them, so nothing but the
    // tool set can.
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
        "the sections arrive in the order the rule names them"
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

#[test]
fn the_persona_names_the_product_the_channel_and_the_reader() {
    let tree = Tree::new();

    let preamble = Preamble::for_workspace(&tree.workspace()).render();

    let persona = preamble.find("## Persona").expect("a persona section");
    let principles = preamble
        .find("## Principles")
        .expect("a principles section");
    assert_eq!(
        preamble[persona..principles].trim_end(),
        "## Persona\n\nYou are Sindri, the agent behind a Unix filter and a terminal UI: you act \
         only through `bash`, and answer in words the reader can use.",
        "the persona names the job as a Unix filter and a terminal UI, the shell it acts through, \
         and the words the reader gets"
    );
}

#[test]
fn the_first_principle_states_the_batching_rule() {
    let tree = Tree::new();

    let preamble = Preamble::for_workspace(&tree.workspace()).render();

    assert_eq!(
        principle_bullets(&preamble).first().copied(),
        Some("- Batch what is independent; chain what depends on what."),
        "the section's first rule says which work to batch and which to chain, and carries no \
         figure of speech where a rule belongs"
    );
}

#[test]
fn the_rule_against_flattery_sits_beside_the_rule_about_what_was_seen() {
    let tree = Tree::new();

    let preamble = Preamble::for_workspace(&tree.workspace()).render();

    let seen = "- Report what you observed; a result you did not see is not a result.";
    let bullets = principle_bullets(&preamble);
    let honesty = bullets
        .iter()
        .position(|bullet| *bullet == seen)
        .expect("the rule about reporting what was seen");
    assert_eq!(
        bullets.get(honesty + 1).copied(),
        Some("- Say what you found, not what pleases: no flattery, no forced enthusiasm."),
        "the rule against pleasing sits immediately after the rule against reporting what was \
         never seen"
    );
}
