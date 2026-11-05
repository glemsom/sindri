//! The lint set the gate runs clippy with, as the workspace declares it: a crash is
//! not a report, so production code may neither `unwrap`, `expect` nor reach a
//! `panic!` on purpose, and a test keeps its own by one `clippy.toml` option each
//! rather than an allow per module. The option is read off the `#[test]` and the
//! `#[cfg(test)]` module and so reaches every test body; the test root whose helper
//! panics for a broken fixture carries one reasoned allow of its own. Nothing else
//! in the tree states this, so a deny dropped from the manifest is a gate that
//! quietly stopped refusing.

/// The workspace manifest, as committed: `include_str!` so the test cannot pass
/// against a file it read from the wrong directory.
const WORKSPACE_MANIFEST: &str = include_str!("../../../Cargo.toml");

/// The clippy configuration, as committed, for the reason the manifest is: the
/// level a lint is denied at and the exemption the tests are given have to be
/// read together, because one without the other refuses the suite's own panics.
const CLIPPY: &str = include_str!("../../../clippy.toml");

/// One `key = value` line of a configuration, with the comment lines stepped over:
/// both files argue their set in prose, and a sentence naming `panic!` is not a
/// deny.
fn states(text: &str, key: &str) -> Option<String> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .find_map(|(named, value)| {
            // A TOML string is quoted and a boolean is not, and both spell the
            // same fact here: whether the file says it.
            (named.trim() == key).then(|| value.trim().trim_matches('"').to_string())
        })
}

/// The level `[workspace.lints.clippy]` denies one lint at, or `None` when the set
/// does not carry it at all — read from that section alone, so a level named in
/// some other table cannot stand in for it.
fn denied(lint: &str) -> Option<String> {
    let section: String = WORKSPACE_MANIFEST
        .split("[workspace.lints.clippy]")
        .nth(1)?
        .lines()
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .fold(String::new(), |mut section, line| {
            section.push_str(line);
            section.push('\n');
            section
        });
    states(&section, lint)
}

#[test]
fn the_gate_denies_a_panic_where_it_denies_an_unwrap() {
    // The family the gate refuses as one thing: a crash is not a report, and
    // reaching one deliberately is the same crash as reaching one by accident.
    for lint in ["unwrap_used", "expect_used", "panic"] {
        assert_eq!(
            denied(lint).as_deref(),
            Some("deny"),
            "`{lint}` is a crash a run would report as nothing, so the workspace \
             denies it for every crate that takes its lints"
        );
    }
    // One `clippy.toml` option beside the other two rather than an allow apiece:
    // a panic in a test is the assertion, exactly as its `unwrap` is.
    for option in [
        "allow-unwrap-in-tests",
        "allow-expect-in-tests",
        "allow-panic-in-tests",
    ] {
        assert_eq!(
            states(CLIPPY, option).as_deref(),
            Some("true"),
            "the tests keep their `{option}`, because the suite reports a broken \
             seam by panicking and an allow in every test module is the same \
             sentence written a hundred times"
        );
    }
}
