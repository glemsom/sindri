//! The crate's build constants, documented as a reader needs them. A doc
//! comment is not observable through the crate's behaviour, so the source file
//! is the seam: `include_str!` rather than a build, so the test cannot pass
//! against a file it read from the wrong directory.

/// The crate root, as committed.
const LIB: &str = include_str!("../src/lib.rs");

/// The `///` lines directly above the line declaring `pub const NAME`, joined
/// into one paragraph. Empty when the constant carries no doc comment.
fn doc_of(name: &str) -> String {
    let declaration = format!("pub const {name}:");
    let lines: Vec<&str> = LIB.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.starts_with(&declaration))
        .unwrap_or_else(|| panic!("src/lib.rs declares no `{declaration}`"));
    let mut above: Vec<&str> = lines[..at]
        .iter()
        .rev()
        .map(|line| line.trim())
        .take_while(|line| line.starts_with("///"))
        .map(|line| line.trim_start_matches("///").trim())
        .collect();
    above.reverse();
    above.join(" ")
}

/// Every `pub const` at the crate root, in the order they are declared.
fn constants() -> Vec<String> {
    LIB.lines()
        .filter_map(|line| line.strip_prefix("pub const "))
        .filter_map(|rest| rest.split_once(':'))
        .map(|(name, _)| name.trim().to_string())
        .collect()
}

#[test]
fn every_build_constant_says_what_it_is() {
    let undocumented: Vec<String> = constants()
        .into_iter()
        .filter(|name| doc_of(name).is_empty())
        .collect();

    assert!(
        undocumented.is_empty(),
        "every constant at the crate root carries a `///` paragraph, so a reader \
         can tell it from its neighbours without reading build.rs: {undocumented:?}"
    );
}

#[test]
fn version_long_says_it_is_the_string_version_prints() {
    let doc = doc_of("VERSION_LONG");

    assert!(
        doc.contains("git describe") && doc.contains("commit date"),
        "it is `git describe` and the commit date as one line: {doc:?}"
    );
    assert!(
        doc.contains("--version"),
        "and the one a user sees, the string `--version` prints: {doc:?}"
    );
    assert!(
        doc.contains("crates/types/build.rs"),
        "it says where it is built, in `crates/types/build.rs`: {doc:?}"
    );
}
