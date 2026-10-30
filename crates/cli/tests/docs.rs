//! The vocabulary, kept true by the two documents that define it: `GLOSSARY.md`
//! names *boot*, and ADR-0015 says the failure's words are the front end's. The
//! front ends in this crate cite both, so a document that drifts from the code
//! makes every comment that cites it a lie (ADR-0008: an agent reads the
//! glossary before it edits code).

/// The glossary, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const GLOSSARY: &str = include_str!("../../../GLOSSARY.md");

const ADR_0015: &str = include_str!("../../../docs/adr/0015-one-module-starts-a-run.md");

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
fn adr_0015_says_the_words_are_the_front_ends_and_claims_no_third_config_door() {
    assert!(
        ADR_0015.contains("10. **The words are the front end's.**"),
        "ADR-0015 carries §10: the one wording per failure, said by every front end"
    );
    assert!(
        ADR_0015.contains("crates/cli/src/failure.rs"),
        "§10 names the module that owns the words, so no front end decides them"
    );
    assert!(
        !ADR_0015.contains("configured_extras"),
        "no third config door: `configured_extras` is deleted, the module has two reads"
    );
}
