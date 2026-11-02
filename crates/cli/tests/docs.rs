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
fn the_glossary_names_running_as_the_records_word_for_a_run_still_going() {
    let entry = GLOSSARY
        .split("\n\n")
        .find(|entry| entry.starts_with("**stopped**"))
        .map(|entry| entry.replace('\n', " "))
        .unwrap_or_else(|| {
            panic!(
                "GLOSSARY.md has no **stopped** entry: stopped names why a run ended \
                 (crates/core/src/agent/report.rs)"
            )
        });

    assert!(
        entry.contains("`running`"),
        "a record writes `stopped: running` until its run ends, so the entry must name \
         the word a reader meets most often: {entry:?}"
    );
    assert!(
        entry.contains("the record's word for a run still going"),
        "`running` is the record's word for a run that has not ended, and the entry \
         must say so in the record's own terms: {entry:?}"
    );
    assert!(
        entry.contains("not a way the loop stops"),
        "the loop's set is closed, so the entry cannot read as though it could stop \
         at `running`: {entry:?}"
    );

    let loops = entry
        .split('.')
        .find(|sentence| sentence.contains("why the loop ended"))
        .unwrap_or_default();
    assert!(
        loops.contains("`error`") && !loops.contains("`running`"),
        "the five words are the loop's own, and `running` stands beside them rather \
         than among them: {loops:?}"
    );
}

#[test]
fn the_readme_names_every_tool_and_points_at_the_records_that_decided_them() {
    // Joined, because the list is wrapped prose: a claim that crosses a line
    // break is one claim, and an absent one is absent either way.
    let opening = README
        .split("\n## ")
        .next()
        .unwrap_or_default()
        .replace('\n', " ");
    let opening = opening.as_str();

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
        opening.contains("`open`"),
        "the frame registers a third tool, and the opening list must name it: {opening:?}"
    );
    for record in [
        "docs/adr/0002-two-tools.md",
        "docs/adr/0003-a-third-tool-open.md",
    ] {
        assert!(
            opening.contains(record),
            "the decision lives in `{record}`, and the opening list must point a reader \
             at it: {opening:?}"
        );
    }
    assert!(
        !opening.contains("get `bash` and nothing else"),
        "the run hands the model three tools, so the opening list cannot say one: \
         {opening:?}"
    );
    assert!(
        opening.contains("**Three tools.**"),
        "the count is the first thing in the list and it is three: {opening:?}"
    );
    for stale in ["**Two tools.**", "**One tool.**", "both tools"] {
        assert!(
            !opening.contains(stale),
            "the opening list counts three, so it cannot still say {stale:?}: \
             {opening:?}"
        );
    }
    assert!(
        opening.contains("host was asked") && opening.contains("never that you looked"),
        "the third tool's result says `started` and never that a human looked, and \
         the opening list must say so: {opening:?}"
    );
    assert!(
        opening.contains("URL") && opening.contains("a file in the workspace"),
        "the third tool takes a **target**, a URL or a file, and the opening list \
         cannot name only the URL: {opening:?}"
    );
}

/// One glossary entry, its lines joined so an assertion reads a sentence
/// rather than a wrap.
fn entry(word: &str) -> String {
    GLOSSARY
        .split("\n\n")
        .find(|entry| entry.starts_with(&format!("**{word}**")))
        .map(|entry| entry.replace('\n', " "))
        .unwrap_or_else(|| panic!("GLOSSARY.md has no **{word}** entry"))
}

#[test]
fn the_glossary_defines_open_target_and_started_as_the_records_do() {
    let open = entry("open");
    assert!(
        open.contains("the third tool") && open.contains("host's opener"),
        "`open` is the third tool and hands a target to the host's opener: {open:?}"
    );
    assert!(
        open.contains("an absence, not a refusal"),
        "a batch run has no `open` in its schema, which is an absence and not a \
         refusal: {open:?}"
    );

    let target = entry("target");
    for claim in ["exactly one of them", "`url`", "`path`", "`Target`"] {
        assert!(
            target.contains(claim),
            "`target` is what one `open` call names — {claim:?} — and the entry must \
             say so: {target:?}"
        );
    }

    let started = entry("started");
    assert!(
        started.contains("the word an `open` result carries"),
        "`started` is the result's word for the host being asked: {started:?}"
    );
    assert!(
        started.contains("Neither says a human looked"),
        "`started` never claims the human looked, and the entry must say so: \
         {started:?}"
    );
    for synonym in ["launched", "displayed", "shown in the browser"] {
        assert!(
            !started.contains(synonym),
            "the result's word is `started` and not {synonym:?}, which would say more \
             than the host can know: {started:?}"
        );
    }
}

#[test]
fn the_doctor_says_xdg_open_is_load_bearing_and_why_its_output_never_changes() {
    let doctor = include_str!("../../../crates/core/src/doctor.rs");
    let note = doctor
        .split("pub const REQUIRED_BINARIES")
        .next()
        .unwrap_or_default();
    assert!(
        note.contains("load-bearing") && note.contains("`xdg-open`"),
        "`xdg-open` is a binary the `open` tool's host runs and not a claim the \
         system prompt makes, and the list's comment must say so: {note:?}"
    );
    assert!(
        note.contains("per-call") && note.contains("graphical session"),
        "a machine with no graphical session is an honest per-call failure from the \
         probe window, which is why `doctor` prints nothing new: {note:?}"
    );
    assert!(
        note.contains("this call"),
        "the failure is a fact about this call and not about the machine, and the \
         comment must say so: {note:?}"
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

#[test]
fn the_readme_states_that_a_repositorys_agents_md_reaches_the_model() {
    let configuration = README
        .split("\n## ")
        .find(|section| section.starts_with("Configuration"))
        .unwrap_or_else(|| panic!("README.md has no `## Configuration` section"));
    let paragraph = configuration
        .split("\n\n")
        .find(|paragraph| paragraph.contains("`AGENTS.md`"))
        .map(|paragraph| paragraph.replace('\n', " "))
        .unwrap_or_else(|| {
            panic!(
                "the Configuration section never names `AGENTS.md`: a user who writes \
                 house rules has no way to learn that they arrive: {configuration:?}"
            )
        });

    assert!(
        paragraph.contains("8 KiB"),
        "the cap is what bounds the bill, and the paragraph must state it: {paragraph:?}"
    );
    assert!(
        paragraph.contains("still starts"),
        "an unreadable file is a diagnostic that never stops the run, and the paragraph \
         must say so: {paragraph:?}"
    );
    assert!(
        paragraph.contains("(GLOSSARY.md)") && paragraph.contains("`instructions`"),
        "the glossary defines the word, and the paragraph must link its `instructions` \
         entry: {paragraph:?}"
    );
}
