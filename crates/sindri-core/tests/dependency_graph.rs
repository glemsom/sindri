//! Proves the core cannot link a terminal, by walking the dependency graph `cargo metadata`
//! resolves rather than trusting a comment: every edge kind counts, dev-dependencies included, and
//! the resolve is filtered to the pinned host so the check stays offline.

use std::collections::{HashMap, VecDeque};
use std::process::Command;

use serde_json::Value;

const BANNED: [&str; 3] = ["ratatui", "clap", "crossterm"];

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
        let id = self
            .names
            .iter()
            .find(|(_, package)| package.as_str() == name)
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("the workspace resolves no package named {name}"));
        id.as_str()
    }

    fn paths_to_banned(&self, root: &str) -> Vec<String> {
        let root = self.id_of(root).to_owned();
        let mut came_from: HashMap<String, Option<String>> = HashMap::new();
        came_from.insert(root.clone(), None);
        let mut queue = VecDeque::from([root.clone()]);

        while let Some(id) = queue.pop_front() {
            for next in self.edges.get(&id).into_iter().flatten() {
                if !came_from.contains_key(next) {
                    came_from.insert(next.clone(), Some(id.clone()));
                    queue.push_back(next.clone());
                }
            }
        }

        let mut paths: Vec<String> = came_from
            .iter()
            .filter(|(id, _)| {
                self.names
                    .get(*id)
                    .is_some_and(|name| BANNED.contains(&name.as_str()))
            })
            .map(|(id, _)| {
                let mut crumbs = vec![id.clone()];
                let mut cursor = id;
                while let Some(Some(previous)) = came_from.get(cursor) {
                    crumbs.push(previous.clone());
                    cursor = previous;
                }
                crumbs.reverse();
                let names: Vec<&str> = crumbs.iter().map(|id| self.names[id].as_str()).collect();
                names.join(" -> ")
            })
            .collect();
        paths.sort();
        paths
    }
}

#[test]
fn core_reaches_no_terminal_and_no_argument_parser() {
    let metadata = workspace_metadata();
    let graph = resolved_graph(&metadata);

    let offending = graph.paths_to_banned("sindri-core");

    assert!(
        offending.is_empty(),
        "sindri-core must reach neither a terminal nor an argument parser:\n  {}",
        offending.join("\n  ")
    );
}
