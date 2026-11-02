//! The vocabulary, kept true by `GLOSSARY.md`: it names *boot*, and the front
//! ends in this crate use that word. Documents drift, so the entry an agent
//! reads first is asserted here rather than trusted.

use sindri_tui::command::COMMANDS;

/// The glossary, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const GLOSSARY: &str = include_str!("../../../GLOSSARY.md");

/// The README, as committed: the opening list of properties is the first thing a
/// reader deciding whether to write a skill reads, so it is asserted rather than
/// trusted to keep up with the run.
const README: &str = include_str!("../../../README.md");

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

#[test]
fn the_glossary_defines_instructions_as_the_repositorys_own_rules() {
    let entry = GLOSSARY
        .split("\n\n")
        .find(|entry| entry.starts_with("**instructions**"))
        .map(|entry| entry.replace('\n', " "))
        .unwrap_or_else(|| {
            panic!(
                "GLOSSARY.md has no **instructions** entry: instructions names the \
                 repository's rules a run hands the model (crates/core/src/instructions.rs)"
            )
        });

    assert!(
        entry.contains("a repository's own house rules"),
        "the entry says what they are: {entry:?}"
    );
    assert!(
        entry.contains("never as a turn of their own"),
        "they arrive beside the preamble and not as a user turn, and the entry must say \
         so: {entry:?}"
    );
    assert!(
        entry.contains("cut with a marker"),
        "a file past the cap arrives cut with a marker rather than silently short: \
         {entry:?}"
    );
    assert!(
        entry.contains("`Taken`"),
        "and the word the code uses is the one the glossary carries: {entry:?}"
    );
}

#[test]
fn the_readme_names_both_tools_and_points_at_the_record_that_decided_it() {
    let opening = README.split("\n## ").next().unwrap_or_default();

    assert!(
        opening.contains("`bash`"),
        "the opening list names the tool that runs everything: {opening:?}"
    );
    assert!(
        opening.contains("`skill`"),
        "the run registers a second tool whenever the workspace ships one, and the \
         opening list must name it: {opening:?}"
    );
    assert!(
        opening.contains("docs/adr/0002-two-tools.md"),
        "the decision lives in ADR 0002, and the opening list must point a reader at \
         it: {opening:?}"
    );
    assert!(
        !opening.contains("get `bash` and nothing else"),
        "the run hands the model two tools, so the opening list cannot say one: \
         {opening:?}"
    );
}

#[test]
fn the_readme_names_every_command_the_composer_can_send() {
    let bullet = README
        .split("\n- **")
        .find(|bullet| bullet.starts_with("`/`**"))
        .map(|bullet| bullet.replace('\n', " "))
        .unwrap_or_else(|| panic!("README.md has no `/` bullet: the composer sends six commands"));

    for spec in COMMANDS {
        assert!(
            bullet.contains(&format!("/{}", spec.name)),
            "the `/` bullet must name `/{}`, or a reader never learns the composer can \
             send it: {bullet:?}",
            spec.name
        );
    }
    assert!(
        bullet.contains("/skill:") && bullet.contains("/skill "),
        "`/skill` is the one name a colon may end, and both spellings are what a user \
         types: {bullet:?}"
    );
    assert!(
        bullet.contains("catalog"),
        "the name it takes is one the catalog offers, and the bullet must say so: \
         {bullet:?}"
    );
}
