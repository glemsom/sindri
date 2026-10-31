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
fn the_glossary_defines_mark_as_the_cells_a_drag_lights_and_the_words_that_leave() {
    let entry = GLOSSARY
        .split("\n\n")
        .find(|entry| entry.starts_with("**mark**"))
        .map(|entry| entry.replace('\n', " "))
        .unwrap_or_else(|| {
            panic!(
                "GLOSSARY.md has no **mark** entry: mark names what a drag lights and \
                 what the copy takes (crates/tui/src/mark.rs)"
            )
        });

    assert!(
        entry.contains("the cells a drag lights and the words that leave with them"),
        "the entry defines both halves of the word, the wash and the copy: {entry:?}"
    );
    assert!(
        entry.contains("replace it with a mark of their own"),
        "`Shift`+`↑`/`↓` replace the drag's mark rather than lay a second one over it, \
         and the entry must say so: {entry:?}"
    );
}
