//! The vocabulary, kept true by `GLOSSARY.md`: every entry names a word the
//! code carries, and the defaults, closed sets and identifiers it points at are
//! read from the code rather than trusted to keep up. An entry is pinned to its
//! fact and not to a sentence, so the file can be rewritten without the test
//! moving — and the test still fails when the fact drifts.

use sindri_core::agent::Stopped;
use sindri_core::config::{DEFAULT_MAX_TURNS, KEYS};
use sindri_core::skills::{DESCRIPTION_CEILING, Scope};
use sindri_tui::command::COMMANDS;
use sindri_types::ExitCode;

/// The glossary, as committed: `include_str!` so the test cannot pass against a
/// file it read from the wrong directory.
const GLOSSARY: &str = include_str!("../../../GLOSSARY.md");

/// The README, as committed: the opening list of properties is the first thing a
/// reader deciding whether to write a skill reads, so it is asserted rather than
/// trusted to keep up with the run.
const README: &str = include_str!("../../../README.md");

/// The tool set's record, as committed: the one argument for `bash` against the two
/// tools the shell cannot carry, since ADR 0003 was folded into it.
const TOOLS: &str = include_str!("../../../docs/adr/0002-two-tools.md");

/// ADR 0003, as committed: superseded, so it stays for the links that point at it
/// and is a pointer to the section that replaced it — never a second copy.
const THIRD_TOOL: &str = include_str!("../../../docs/adr/0003-a-third-tool-open.md");

/// The code's own record, as committed: each bullet points at the script that holds
/// the rule it states, and a wrong pointer is a reader searching the wrong file.
const CODE_RULES: &str = include_str!("../../../docs/adr/0001-llm-friendly-code.md");

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
    let record = "docs/adr/0002-two-tools.md";
    assert!(
        opening.contains(record),
        "the decision lives in `{record}`, and the opening list must point a reader \
         at it: {opening:?}"
    );
    assert!(
        !opening.contains("docs/adr/0003-a-third-tool-open.md"),
        "ADR 0003 is superseded and folded into `{record}`, so the opening list sends a \
         reader to the record that holds the argument and not to the pointer: {opening:?}"
    );
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

#[test]
fn the_superseded_record_points_at_the_one_that_replaced_it() {
    assert!(
        THIRD_TOOL.starts_with("---\nstatus: superseded\n"),
        "ADR 0003 was folded into ADR 0002, so its status says so at the top of the \
         file: {:?}",
        THIRD_TOOL.lines().take(4).collect::<Vec<_>>()
    );
    assert!(
        THIRD_TOOL.contains("docs/adr/0002-two-tools.md"),
        "a superseded record is a pointer, so it names the record it was folded into"
    );
    assert!(
        TOOLS.contains("xdg-open") && TOOLS.contains("`started`") && TOOLS.contains("https"),
        "the argument 0003 made is the one 0002 must now carry: the browser is not a \
         `bash` command, and the result says `started` and never that a human looked"
    );
    let lines = THIRD_TOOL.lines().count();
    assert!(
        lines <= 20,
        "a pointer and not a copy — the failure ADR 0004's duplication rule forbids — is \
         a few lines, and this one is {lines}"
    );
}

#[test]
fn the_line_length_rule_names_the_script_that_holds_its_scope() {
    let bullet = CODE_RULES
        .split("\n- **")
        .find(|bullet| bullet.starts_with("Rust modules stay small"))
        .map(|bullet| bullet.replace('\n', " "))
        .unwrap_or_else(|| {
            panic!("ADR 0001 has no line-length bullet: it is the rule every module is measured against")
        });
    assert!(
        bullet.contains("scripts/tests/line-length.sh"),
        "the scope rule — `*.rs` measured, `*.sh` and `*.md` outside it — is held by the \
         line-length test, and the bullet must point there: {bullet:?}"
    );
    assert!(
        !bullet.contains("module-docs"),
        "the module-docs test asserts what a file opens with and says nothing about \
         which files are measured, so it is the wrong script to send a reader to: \
         {bullet:?}"
    );
}

/// One glossary entry, its lines joined so an assertion reads a claim rather
/// than a wrap.
fn entry(word: &str) -> String {
    GLOSSARY
        .split("\n\n")
        .find(|entry| entry.starts_with(&format!("**{word}**")))
        .map(|entry| entry.replace('\n', " "))
        .unwrap_or_else(|| panic!("GLOSSARY.md has no **{word}** entry"))
}

/// One word the loop can stop in, spelled by the code. The match carries no
/// wildcard, so a sixth word stops every test that stops a run compiling until
/// the glossary and this list carry it too.
fn stop_word(stopped: Stopped) -> &'static str {
    match stopped {
        Stopped::Final
        | Stopped::MaxTurns
        | Stopped::Budget
        | Stopped::Cancelled
        | Stopped::Error => stopped.as_str(),
    }
}

#[test]
fn the_glossary_carries_the_turn_cap_the_code_defaults_to() {
    let turn = entry("turn");
    assert!(
        turn.contains("`max_turns`"),
        "the cap is set by `max_turns`, and the entry must name it: {turn:?}"
    );
    assert!(
        turn.contains(&DEFAULT_MAX_TURNS.to_string()),
        "the cap the code defaults to is {DEFAULT_MAX_TURNS}, and the entry must carry \
         that number rather than one of its own: {turn:?}"
    );
}

#[test]
fn the_glossary_carries_every_word_the_loop_can_stop_in() {
    let stopped = entry("stopped");
    for word in [
        stop_word(Stopped::Final),
        stop_word(Stopped::MaxTurns),
        stop_word(Stopped::Budget),
        stop_word(Stopped::Cancelled),
        stop_word(Stopped::Error),
    ] {
        assert!(
            stopped.contains(&format!("`{word}`")),
            "`Stopped` spells the word `{word}`, and the entry that defines the closed \
             set must carry it: {stopped:?}"
        );
    }
    assert!(
        stopped.contains("`running`"),
        "a record writes `running` from the moment it starts, so the entry must name \
         the word a reader meets most often: {stopped:?}"
    );
}

#[test]
fn the_glossary_orders_the_skill_scopes_as_the_scan_reads_them() {
    let scope = entry("skill scope");
    let mut after = 0;
    for placed in [Scope::Builtin, Scope::User, Scope::Repo] {
        let word = format!("`{}`", placed.as_str());
        let at = scope.find(&word).unwrap_or_else(|| {
            panic!(
                "`Scope` spells a scope `{}`, and the entry must carry it: {scope:?}",
                placed.as_str()
            )
        });
        assert!(
            at >= after,
            "the scopes are ordered by nearness, {} first, and the entry must list them \
             in that order: {scope:?}",
            Scope::Builtin.as_str()
        );
        after = at + word.len();
    }
}

#[test]
fn the_glossary_carries_the_description_ceiling_the_refusal_names() {
    let ceiling = entry("description ceiling");
    assert!(
        ceiling.contains("`DESCRIPTION_CEILING`"),
        "the entry names the constant a refusal states: {ceiling:?}"
    );
    assert!(
        ceiling.contains(&DESCRIPTION_CEILING.to_string()),
        "the ceiling is {DESCRIPTION_CEILING}, and the entry must carry that number: \
         {ceiling:?}"
    );
}

/// Every entry that points at the tree: the word, the identifier it points
/// with, and the file that identifier is declared in — read as `include_str!`,
/// so an entry pointing at a deleted module stops this test compiling.
const POINTED_AT: [(&str, &str, &str, &str); 4] = [
    (
        "boot",
        "crates/cli/src/boot.rs",
        include_str!("../src/boot.rs"),
        "pub struct Boot",
    ),
    (
        "mark",
        "`mark::span`",
        include_str!("../../../crates/tui/src/mark.rs"),
        "pub fn span",
    ),
    (
        "block",
        "`Viewport::block`",
        include_str!("../../../crates/tui/src/selection.rs"),
        "pub fn block",
    ),
    (
        "instructions",
        "`Taken`",
        include_str!("../../../crates/core/src/instructions.rs"),
        "pub enum Taken",
    ),
];

#[test]
fn the_glossary_points_each_word_at_the_identifier_the_tree_carries() {
    for (word, identifier, source, declared) in POINTED_AT {
        let entry = entry(word);
        assert!(
            entry.contains(identifier),
            "the entry for {word:?} must name {identifier:?}, the word the code uses \
             for it: {entry:?}"
        );
        assert!(
            source.contains(declared),
            "the entry for {word:?} points at {identifier:?}, which must still be \
             declared {declared:?} in the file it names"
        );
    }
}

#[test]
fn the_glossary_tells_the_drag_and_copy_rule_in_one_entry_and_points_the_others_at_it() {
    let mark = entry("mark");
    assert!(
        mark.contains("`mark::span`"),
        "the one function the wash and the copy both read is the mark's own: {mark:?}"
    );
    let block = entry("block");
    assert!(
        block.contains("**mark**"),
        "`block` is a **picture**'s rows, and what a copy takes from one is **mark**'s \
         rule, told once there: {block:?}"
    );
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

/// The place the README must name a **skill scope** by, beside the word `doctor`
/// prints for it. The match carries no wildcard, so a fourth scope fails this
/// test's build until the paragraph is given somewhere to say it lives.
fn scope_place(scope: Scope) -> &'static str {
    match scope {
        Scope::Builtin => "compiled into the binary",
        Scope::User => "`~/.agents/skills`",
        Scope::Repo => "each `.agents/skills` from the workspace up to its repository root",
    }
}

#[test]
fn the_readme_names_every_scope_the_catalog_reads() {
    let install = README
        .split("\n## ")
        .find(|section| section.starts_with("Install"))
        .unwrap_or_else(|| panic!("README.md has no `## Install` section"));
    let paragraph = install
        .split("\n\n")
        .find(|paragraph| paragraph.contains("`doctor`") && paragraph.contains("skill"))
        .map(|paragraph| paragraph.replace('\n', " "))
        .unwrap_or_else(|| {
            panic!("the Install section never says what `doctor` lists: {install:?}")
        });

    // A reader deciding whether a skill compiled into the binary exists is who
    // opens this file, so the lowest scope is named beside the two a reader can
    // find on disk rather than left for `sindri doctor` to spring on them.
    for scope in [Scope::Builtin, Scope::User, Scope::Repo] {
        let word = format!("`{}`", scope.as_str());
        assert!(
            paragraph.contains(&word),
            "the catalog reads the `{}` scope and `doctor` prints that word for it, so \
             the paragraph must name it: {paragraph:?}",
            scope.as_str()
        );
        assert!(
            paragraph.contains(scope_place(scope)),
            "the `{}` scope is found where the paragraph must say, at {:?}: \
             {paragraph:?}",
            scope.as_str(),
            scope_place(scope)
        );
    }
}

/// The words the README's exit table gives one code. Exhaustive for the reason
/// `scope_place` is: a code the table has not learned stops this test compiling.
fn exit_meaning(code: ExitCode) -> &'static str {
    match code {
        ExitCode::Success => "an answer",
        ExitCode::ModelError => "model or provider error",
        ExitCode::UsageError => "usage error",
        ExitCode::EnvironmentFailure => "sandbox or setup failure",
        ExitCode::TurnCap => "turn cap",
        ExitCode::Budget => "spent its command time or its output",
        ExitCode::Cancelled => "cancelled, nothing on stdout",
    }
}

#[test]
fn the_readme_exit_table_carries_every_code_a_run_can_leave_with() {
    let batch = README
        .split("\n## ")
        .find(|section| section.starts_with("Batch mode"))
        .unwrap_or_else(|| panic!("README.md has no `## Batch mode` section"));
    let table = batch
        .split("\n\n")
        .find(|block| block.starts_with("| Exit |"))
        .unwrap_or_else(|| panic!("the Batch mode section has no exit table: {batch:?}"));

    for code in [
        ExitCode::Success,
        ExitCode::ModelError,
        ExitCode::UsageError,
        ExitCode::EnvironmentFailure,
        ExitCode::TurnCap,
        ExitCode::Budget,
        ExitCode::Cancelled,
    ] {
        let row = format!("| `{}` | {} ", code.code(), exit_meaning(code));
        assert!(
            table.contains(&row),
            "the run leaves with `{}` for {:?}, and a script branching on it must read \
             that in the table: {table:?}",
            code.code(),
            exit_meaning(code)
        );
    }
}

#[test]
fn the_readme_configuration_table_names_every_key_the_schema_holds() {
    let configuration = README
        .split("\n## ")
        .find(|section| section.starts_with("Configuration"))
        .unwrap_or_else(|| panic!("README.md has no `## Configuration` section"));

    for key in KEYS {
        let named = format!("`{}`", key.name());
        assert!(
            configuration.contains(&named),
            "`KEYS` holds a key the file carries, and the table must name it or a \
             setting exists that no reader can set: {named} in {configuration:?}"
        );
    }
}

/// The program-status record, as committed: the mapping it states is
/// `Stopped`'s, so the words it maps are read from the code rather than
/// trusted to keep up.
const PROGRAM_STATUS: &str = include_str!("../../../docs/adr/0005-program-status.md");

/// The state one way of stopping reports, in the words the record uses.
fn status_for(stopped: Stopped) -> &'static str {
    match stopped {
        Stopped::Final | Stopped::MaxTurns | Stopped::Budget => "done",
        Stopped::Cancelled => "idle",
        Stopped::Error => "error",
    }
}

#[test]
fn the_status_record_maps_every_word_the_loop_can_stop_in() {
    for stopped in [
        Stopped::Final,
        Stopped::MaxTurns,
        Stopped::Budget,
        Stopped::Cancelled,
        Stopped::Error,
    ] {
        let word = stop_word(stopped);
        let state = status_for(stopped);
        for quoted in [word, state] {
            assert!(
                PROGRAM_STATUS.contains(&format!("`{quoted}`")),
                "`Stopped` spells `{word}`, which reports `{state}`, and the record \
                 must carry both words: `{quoted}` is missing"
            );
        }
    }
    assert!(
        PROGRAM_STATUS.contains("No `progress`"),
        "the specification makes an absent `progress` mean indeterminate, and the \
         record must refuse the key rather than leave a reader to add it"
    );
}

#[test]
fn the_status_record_names_the_encoder_and_the_file_each_front_end_writes_from() {
    for named in [
        "crates/core/src/status.rs",
        "`status::report`",
        "crates/tui/src/run.rs",
        "crates/cli/src/batch.rs",
        "`OSC 7501`",
    ] {
        assert!(
            PROGRAM_STATUS.contains(named),
            "the encoder and the two files that write from it are what a front end \
             is handed, and the record must name {named:?}"
        );
    }
    for refused in ["`OSC 7501 ; ?`", "`9;4`", "`blocked`", "`id=`"] {
        assert!(
            PROGRAM_STATUS.contains(refused),
            "the report is unqueried, not `9;4`, never `blocked` and root-only, and \
             the record must say so about {refused:?}"
        );
    }
}

#[test]
fn the_glossary_carries_the_states_a_status_can_be() {
    let status = entry("status");
    for state in ["idle", "working", "done", "blocked", "error"] {
        assert!(
            status.contains(&format!("`{state}`")),
            "the set of states is the specification's and it is closed, so the entry \
             must carry `{state}`: {status:?}"
        );
    }
    for named in [
        "`sindri_core::status::State`",
        "`status::report`",
        "`app=sindri`",
    ] {
        assert!(
            status.contains(named),
            "the entry points at the word the code uses for it, so it must name \
             {named:?}: {status:?}"
        );
    }
}

#[test]
fn the_glossary_says_which_sense_of_record_is_which() {
    let record = entry("record");
    assert!(
        record.contains("**status**") && record.contains("terminal"),
        "the specification's *record* is what a terminal stores from a **status** \
         report, and the entry for the older word must say so: {record:?}"
    );
    assert!(
        record.contains("`agent::Report`"),
        "`agent::Report` is the third sense of the word — what one run did — and the \
         entry must name it: {record:?}"
    );
}

#[test]
fn a_glossary_entry_does_not_reuse_a_word_another_one_holds() {
    let mut seen: Vec<String> = Vec::new();
    for block in GLOSSARY.split("\n\n") {
        let Some(rest) = block.strip_prefix("**") else {
            continue;
        };
        let Some((word, _)) = rest.split_once("**") else {
            continue;
        };
        assert!(
            !seen.iter().any(|held| held == word),
            "the glossary holds one entry per word, and `{word}` has two: a second \
             meaning needs its own word, or the drift this file exists to stop"
        );
        seen.push(word.to_owned());
    }
    assert!(
        seen.len() > 60,
        "the scan reads the file it is pointed at, and `{}` entries is too few for \
         that to be the glossary",
        seen.len()
    );
}
