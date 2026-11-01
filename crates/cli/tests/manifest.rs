//! The CLI crate's dependencies, as its manifest declares them. Cargo already
//! resolves a normal dependency for the unit tests and the integration tests,
//! so a name repeated under `[dev-dependencies]` buys nothing and reads as a
//! claim that the tests need something the binary does not. The manifest is the
//! only place that claim is written, so it is asserted rather than trusted.

/// The manifest, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const MANIFEST: &str = include_str!("../Cargo.toml");

/// The dependency names one `[section]` of the manifest declares, in order. A
/// line's key is what precedes its first `=`, cut at the first `.` so that
/// `sindri-core.workspace = true` names `sindri-core`.
fn declares(section_name: &str) -> Vec<String> {
    let header = format!("[{section_name}]");
    let mut lines = MANIFEST.lines().skip_while(|line| line.trim() != header);
    if lines.next().is_none() {
        panic!("the manifest declares no {header} section");
    }
    lines
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, _)| {
            // `.workspace = true` and friends make the key a path, not a name.
            key.split_once('.')
                .map_or(key, |(name, _)| name)
                .trim()
                .to_string()
        })
        .collect()
}

#[test]
fn a_dependency_the_binary_already_declares_is_not_repeated_for_the_tests() {
    let dependencies = declares("dependencies");
    let dev = declares("dev-dependencies");

    // A normal dependency is resolved for every target the crate builds, tests
    // and doctests included, so repeating one under `[dev-dependencies]` is a
    // line that cannot change the build.
    for name in &dev {
        assert!(
            !dependencies.contains(name),
            "`{name}` is declared under `[dependencies]` already, so the binary \
             depends on it and Cargo resolves it for the tests too"
        );
    }
}

#[test]
fn the_binary_depends_on_sindri_core() {
    // The redundant line is the one under `[dev-dependencies]`: `sindri-core` is
    // the binary's own dependency and stays where the binary asks for it.
    assert!(
        declares("dependencies")
            .iter()
            .any(|name| name == "sindri-core"),
        "`sindri-core` is a dependency of the binary, not only of its tests"
    );
}
