//! The vocabulary, kept true by `GLOSSARY.md`: it names *boot*, and the front
//! ends in this crate use that word. Documents drift, so the entry an agent
//! reads first is asserted here rather than trusted.

/// The glossary, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const GLOSSARY: &str = include_str!("../../../GLOSSARY.md");

#[test]
fn the_glossary_defines_boot_as_the_turn_from_flags_and_process_into_a_session() {
    let entry = GLOSSARY
        .split("\n\n")
        .find(|entry| entry.starts_with("**boot**"))
        .map(|entry| entry.replace('\n', " "))
        .unwrap_or_else(|| {
            panic!(
                "GLOSSARY.md has no **boot** entry: boot names the module every \
                 front end boots through (crates/cli/src/boot.rs)"
            )
        });

    assert!(
        entry.contains("turning the flags and the process's own facts into a session"),
        "the entry defines the turn, in the issue's own words: {entry:?}"
    );
    assert!(
        entry.contains("synonym for **session**"),
        "boot ends where a session begins, and the glossary must say so: {entry:?}"
    );
}

#[test]
fn the_one_adr_is_a_rulebook_an_agent_can_check() {
    let adr = include_str!("../../../docs/adr/0001-code-an-agent-can-maintain.md");
    assert!(
        adr.contains("## Decision"),
        "the ADR states its rules under one heading"
    );
    assert!(
        adr.contains("`make check` is the definition of done"),
        "it names the gate, so an agent knows what done means"
    );
    assert!(
        adr.contains("New decisions are not new ADRs"),
        "a new decision goes in code, a test or the glossary, not in a new record"
    );
}
