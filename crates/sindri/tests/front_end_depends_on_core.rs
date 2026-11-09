//! The edge that makes "no front end owns a decision the core could make" a compile error rather than a
//! convention. The reverse direction is checked on direct edges, which suffices: cargo forbids a cycle,
//! and a path from the core back to the front end would close one.

use std::collections::HashMap;
use std::process::Command;

use serde_json::Value;

/// The devbox toolchain's platform, so the resolve stays offline.
const PLATFORM: &str = "x86_64-unknown-linux-gnu";

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
