//! The preamble every turn carries: Sindri's stance in its own voice, the
//! Environment section whose one derived fact is the workspace the run stands
//! in, and a repository's instructions when it has any.
//!
//! Three layers carry what the model is told, and each holds only its own kind
//! of fact. The **stance** is here: who is answering and the principles it works
//! by. The **environment** is here too, because the habits that keep a run cheap
//! — staged commands, one call for one question, `$TMPDIR` for whatever it makes
//! — are the run's own, and the one path they resolve against is derived per
//! run rather than typed. The **procedure** is a skill's body: the steps of one
//! task, loaded once and only where a skill is offered. The **boundary** is the
//! `bash` description, derived from the sandbox policy in force: the paths that
//! may be written, the timeout, the session temp. A fact in the wrong layer is
//! paid for twice on every turn of every run and is a second place for it to
//! disagree with the one that is enforced, so the split is stated here beside
//! the code that depends on it rather than in a record of its own.
//!
//! Both shipped texts are literals a benchmark can read back out of this file
//! (`scripts/bench-system-prompt`), so they are spelled as one escaped literal
//! each and never composed from pieces.

use std::path::Path;

/// Sindri's stance: who answers, and the principles that shape an answer. Every
/// word is re-sent on every turn of every run, so a line here has to change what
/// a run does; the procedure for a task is a skill body's, loaded once.
const STANCE: &str = "## Persona\n\
    You are Sindri, a dwarven smith made digital — an AI assistant that can forge anything: code, \
    prose, analysis, plans. You work in a GNU/Linux workspace through `bash`.\n\n\
    ## Principles\n\
    - **Smith it:** one clean strike beats a flurry — but heat every bar in the forge at once; \
    full substance, no filler. Be concise.\n\
    - **Simplest correct solution:** focused edits over full rewrites; match the surrounding code \
    style.\n\
    - **Batch the independent, chain the dependent.** Overlap independent work — several `bash` \
    calls in one turn (they run concurrently) or `cmd & … & wait` in one call; use `&&`/pipes only \
    when a stage needs the prior output. Fewer turns = fewer tokens and less waiting.\n\
    - Prefer command-line tools; write a script when state or control flow requires it.";

/// The Environment section: the habits of this machine, and the one path they
/// resolve against. `{workspace}` is the only placeholder in either shipped
/// text, and it is filled from the run rather than typed — a path written here
/// would be a path that can be wrong the moment a run starts elsewhere.
const ENVIRONMENT: &str = "## Environment\n\
    - **GNU/Linux, `bash` first**: use available tools (`coreutils`, `rg`, `git`, `python3`, \
    `curl`, `jq`, etc.).\n\
    - For multi-stage commands, prefix each stage with `echo 'STEP: <what>'`; use `set -o \
    pipefail` to expose failures, and `||` for intentional fallbacks.\n\
    - Downloads, generated files, temp scripts, rendered HTML etc goes to `$TMPDIR`; `/tmp` is \
    read-only, never hard-code it.\n\
    - Current working directory: {workspace}";

/// The preamble as one value: what [`of`] composes and the only thing a client
/// is handed, so no front end assembles a system message of its own.
#[derive(Debug, Clone)]
pub struct Preamble(String);

impl Preamble {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The preamble a run sends: [`STANCE`], the Environment section, and a
/// repository's own instructions after them when the run stands in one that has
/// any.
///
/// The Environment section holds the habits of the machine and one fact — the
/// workspace the run was started in, derived from the session, because every
/// relative path in a call resolves there and a typed path is a path that can be
/// wrong. What may be written, the timeout and the session temp are the `bash`
/// description's facts, read off the policy in force; a copy here would be a
/// boundary stated twice and enforced once.
///
/// The instructions are one message with the preamble rather than a turn of
/// their own, so the transcript a user reads is their own words. What follows
/// the preamble is a file, and its size is [`crate::instructions::CAP`]'s
/// business, never the stance's.
pub fn of(workspace: &Path, instructions: Option<&str>) -> Preamble {
    let environment = ENVIRONMENT.replace("{workspace}", &workspace.display().to_string());
    let mut preamble = format!("{STANCE}\n\n{environment}\n");
    if let Some(text) = instructions {
        // The section's own line ends in a newline, so this one is the blank
        // line between it and a file someone else wrote.
        preamble.push('\n');
        preamble.push_str(text);
    }
    Preamble(preamble)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ENVIRONMENT, STANCE, of};
    use crate::agent::Silent;

    /// A workspace path as a run holds one: absolute, and nothing this file
    /// could have known before the run started.
    const WORKSPACE: &str = "/home/dev/checkout/sindri";

    #[test]
    fn the_stance_opens_the_preamble_and_the_environment_follows_it() {
        let preamble = of(Path::new(WORKSPACE), None);
        let preamble = preamble.as_str();
        assert!(
            preamble.starts_with(STANCE),
            "the stance opens the preamble: {preamble}"
        );
        assert!(
            preamble.contains(&format!(
                "\n\n{}\n",
                ENVIRONMENT.replace("{workspace}", WORKSPACE)
            )),
            "the Environment section follows it whole, filled from the run: {preamble}"
        );
    }

    #[test]
    fn the_environment_names_the_working_directory_the_run_stands_in() {
        let preamble = of(Path::new(WORKSPACE), None);
        let preamble = preamble.as_str();
        assert!(
            preamble.contains(&format!("Current working directory: {WORKSPACE}")),
            "the one fact the run derives is the directory every relative path resolves \
             against: {preamble}"
        );
    }

    #[test]
    fn the_shipped_texts_hold_one_placeholder_and_it_is_the_runs_to_fill() {
        // A path typed into either shipped text is a path that can be wrong; the
        // only workspace the preamble carries is the one a run derives.
        for placeholder in ['{', '}', '<', '$'] {
            assert!(
                !STANCE.contains(placeholder),
                "the stance is a prompt, not a template: {placeholder:?} in {STANCE}"
            );
        }
        assert_eq!(
            ENVIRONMENT.matches("{workspace}").count(),
            1,
            "the Environment section has one placeholder, filled once: {ENVIRONMENT}"
        );
    }

    #[test]
    fn the_environment_holds_the_habits_and_the_stance_holds_the_principles() {
        // Two headings, two kinds of fact: who answers, and how this machine is
        // worked. A habit that drifted into the stance would be a second voice
        // for the same instruction.
        assert!(
            STANCE.starts_with("## Persona\n"),
            "the stance opens with who is answering: {STANCE}"
        );
        assert!(
            STANCE.contains("\n## Principles\n"),
            "and states its principles under their own heading: {STANCE}"
        );
        assert!(
            !STANCE.contains("$TMPDIR") && !STANCE.contains("STEP:"),
            "the machine's habits are the Environment section's: {STANCE}"
        );
        assert!(
            ENVIRONMENT.starts_with("## Environment\n"),
            "the section is named where the model reads it: {ENVIRONMENT}"
        );
    }

    #[test]
    fn the_instructions_follow_the_preamble_whole_and_change_no_word_of_it() {
        let rules = "<project_instructions>\nBe brief, and prove what you claim.\n</project_instructions>\n";

        let alone = of(Path::new(WORKSPACE), None);
        let together = of(Path::new(WORKSPACE), Some(rules));
        let (alone, together) = (alone.as_str(), together.as_str());

        assert_eq!(
            together,
            format!("{alone}\n{rules}"),
            "a repository's own words follow the preamble after one blank line, and its own \
             words are untouched: {together}"
        );
    }

    #[test]
    fn the_preamble_states_no_boundary_the_tool_description_owns() {
        // The writable set, the timeout and the session temp are derived from
        // the policy into the `bash` description. The Environment section names
        // the habits and the directory; it names no path a run may write, because
        // a boundary told twice is enforced once.
        let workspace = tempfile::TempDir::new().expect("workspace");
        let policy = crate::sandbox::SandboxPolicy::fixture(workspace.path());
        let preamble = of(workspace.path(), None);
        let preamble = preamble.as_str();
        let temp = policy.session_temp().display().to_string();

        assert!(
            !preamble.contains(&temp),
            "the session temp path is the description's alone: {preamble}"
        );
        // The workspace is the one path both texts hold, and each says something
        // else about it: the preamble names where the run stands, the description
        // names what may be written. Every other entry is the description's.
        let workspace_path = workspace.path().display().to_string();
        for path in policy.writable().paths() {
            let shown = path.display().to_string();
            if shown == workspace_path {
                continue;
            }
            assert!(
                !preamble.contains(&shown),
                "the writable set is the description's alone, not `{shown}`: {preamble}"
            );
        }
    }

    #[test]
    fn a_pointer_to_the_catalog_is_the_tool_descriptions_and_never_the_preambles() {
        // The nudge to load a skill rides on `bash`, beside the tool the model
        // is already reading, and only where a catalog exists. A second copy in
        // the preamble would be the same nudge paid for on every turn of every
        // run, in a place that cannot know whether there is one.
        let workspace = tempfile::TempDir::new().expect("workspace");
        let policy = crate::sandbox::SandboxPolicy::fixture(workspace.path());
        let bash = crate::agent::BashTool::new(
            &policy,
            crate::agent::Limits::default(),
            std::sync::Arc::new(Silent),
        );
        let shipped = crate::skills::fold([crate::skills::builtins()]);
        let tool = crate::agent::Tools::new(bash, crate::agent::SkillTool::new(&shipped), None);
        let preamble = of(workspace.path(), None);
        let preamble = preamble.as_str();

        assert!(
            tool.specs()[0].description.contains("`skill`"),
            "the description is where the catalog is pointed at: {}",
            tool.specs()[0].description
        );
        assert!(
            !preamble.contains("`skill`"),
            "and the preamble points at nothing: {preamble}"
        );
    }
}
