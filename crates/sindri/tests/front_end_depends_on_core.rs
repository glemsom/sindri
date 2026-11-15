//! The edge that makes "no front end owns a decision the core could make" a compile error rather than a
//! convention. The reverse direction is checked on direct edges, which suffices: cargo forbids a cycle,
//! and a path from the core back to the front end would close one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The devbox toolchain's platform, so the resolve stays offline.
const PLATFORM: &str = "x86_64-unknown-linux-gnu";

/// The parts a run is joined from, which only the core names: a front end that built one would be
/// deciding how a run is assembled.
const JOINED_IN_CORE: [&str; 5] = [
    "Asking::new",
    "Shell::new",
    "Run::new",
    "Record::open",
    "Client::for_endpoint",
];

fn sources(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).expect("the tree is readable");
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

struct Graph {
    names: HashMap<String, String>,
    edges: HashMap<String, Vec<String>>,
}

fn workspace_metadata() -> Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| String::from("cargo"));
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--offline"])
        .args(["--filter-platform", PLATFORM])
        .args(["--manifest-path", manifest])
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata should emit JSON")
}

fn resolved_graph(metadata: &Value) -> Graph {
    let names = metadata["packages"]
        .as_array()
        .expect("packages is a list")
        .iter()
        .map(|package| {
            (
                package["id"].as_str().expect("id is a string").to_owned(),
                package["name"]
                    .as_str()
                    .expect("name is a string")
                    .to_owned(),
            )
        })
        .collect();

    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    for node in metadata["resolve"]["nodes"]
        .as_array()
        .expect("resolve nodes is a list")
    {
        let id = node["id"].as_str().expect("node id is a string").to_owned();
        let deps = node["deps"]
            .as_array()
            .expect("node deps is a list")
            .iter()
            .map(|dep| dep["pkg"].as_str().expect("dep pkg is a string").to_owned())
            .collect();
        edges.insert(id, deps);
    }

    Graph { names, edges }
}

impl Graph {
    fn id_of(&self, name: &str) -> &str {
        self.names
            .iter()
            .find(|(_, package)| package.as_str() == name)
            .map(|(id, _)| id.as_str())
            .unwrap_or_else(|| panic!("the workspace resolves no package named {name}"))
    }

    fn direct_dependencies(&self, name: &str) -> Vec<&str> {
        self.edges
            .get(self.id_of(name))
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|id| self.names[id].as_str())
            .collect()
    }
}

#[test]
fn front_end_depends_on_the_core() {
    let metadata = workspace_metadata();
    let graph = resolved_graph(&metadata);

    let dependencies = graph.direct_dependencies("sindri");

    assert!(
        dependencies.contains(&"sindri-core"),
        "crates/sindri must depend on sindri-core, its dependencies are {dependencies:?}"
    );
}

#[test]
fn core_depends_on_no_front_end() {
    let metadata = workspace_metadata();
    let graph = resolved_graph(&metadata);

    let dependencies = graph.direct_dependencies("sindri-core");

    assert!(
        !dependencies.contains(&"sindri"),
        "sindri-core must not depend on a front end, its dependencies are {dependencies:?}"
    );
}

#[test]
fn no_front_end_module_assembles_a_run() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut named = Vec::new();
    for path in sources(&root) {
        let source = std::fs::read_to_string(&path).expect("a source file is readable");
        for (number, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for part in JOINED_IN_CORE {
                if code.contains(part) {
                    named.push(format!("{}:{}: {part}", path.display(), number + 1));
                }
            }
        }
    }

    assert!(
        named.is_empty(),
        "the core is the only place a run is joined, and these name a part of one: {named:#?}"
    );
}
