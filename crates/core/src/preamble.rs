//! The preamble every turn carries: Sindri's stance in its own voice, the
//! Environment section whose one fact is the workspace the run stands in, and a
//! repository's instructions when it has any.
//!
//! Three layers carry what the model is told, and each holds only its own kind
//! of fact. The **stance** is here: who is answering, in what voice, and the two
//! habits no tool can demonstrate — reading the fewest bytes that answer, and
//! turning many inputs into one script. The **procedure** is a skill's body: the
//! steps of one task, loaded once and only where a skill is offered. The
//! **boundary** is the `bash` description, derived from the sandbox policy in
//! force: the writable set, the session temp, `HOME`, the network. A fact in the
//! wrong layer is paid for twice on every turn of every run and is a second
//! place for it to disagree with the one that is enforced, so the split is
//! stated here beside the code that depends on it rather than in a record of its
//! own.
//!
//! The batching habit is the one line that reads like the `bash` description's
//! sentence about a script, and it is here for the half that description does
//! not say: that description is about the steps of one task, this is about many
//! inputs. The step-by-step procedure stays there, beside the tool that runs it;
//! only the delta is stated here, once.

use std::path::Path;

/// Sindri's stance, measured: told to search and open ranges, a model spent
/// 0.42x the input tokens of the same model told nothing, on the same four tasks
/// with the same answers right — reproducible with `scripts/bench-system-prompt`,
/// whose baseline reads this literal out of this file and whose arms vary it and
/// nothing else: the Environment section is the run's, not the bench's.
const STANCE: &str = "You are Sindri, answering a developer at a Unix terminal. Reply with the \
    answer itself: no lead-in, no closing offer of further help. Run commands rather than \
    guessing. Read the fewest bytes that answer: search first (`rg -n`), then open only the lines \
    you need (`sed -n 'A,Bp'`); never `cat` a whole file, and never run a search or a listing \
    twice. Many inputs are one script: ten files are one loop in one call. A fence marked \
    `mermaid` is drawn as a picture: draw what prose cannot.";

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
/// The Environment section holds one fact — the workspace the run was started in,
/// derived from the session, because every relative path in a call resolves
/// there and a typed path is a path that can be wrong. Nothing else is said
/// about the surroundings: what may be written, where the session temp is and what
/// `HOME` holds are the `bash` description's facts, read off the policy in
/// force; a copy here would be a boundary stated twice and enforced once.
///
/// The instructions are one message with the preamble rather than a turn of
/// their own, so the transcript a user reads is their own words. What follows
/// the preamble is a file, and its size is [`crate::instructions::CAP`]'s
/// business, never the stance's.
pub fn of(workspace: &Path, instructions: Option<&str>) -> Preamble {
    let mut preamble = format!(
        "{STANCE}\n\nEnvironment\n  workspace: {} — relative paths read from here\n",
        workspace.display()
    );
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

    use super::{STANCE, of};
    use crate::agent::Silent;

    /// A workspace path as a run holds one: absolute, and nothing this file
    /// could have known before the run started.
    const WORKSPACE: &str = "/home/dev/checkout/sindri";

    /// The two sentences the stance keeps whatever else it sheds: the reading
    /// rule, measured at 0.42x the tokens of the same model told nothing, and the
    /// renderer fact no model can know — quoted from the shipped text.
    const READING_RULE: &str = "Read the fewest bytes that answer: search first (`rg -n`), then open \
        only the lines you need (`sed -n 'A,Bp'`); never `cat` a whole file, and never run a search \
        or a listing twice.";
    const MERMAID: &str = "A fence marked `mermaid` is drawn as a picture: draw what prose cannot.";

    #[test]
    fn the_environment_section_names_the_workspace_the_run_stands_in() {
        let preamble = of(Path::new(WORKSPACE), None);
        let preamble = preamble.as_str();
        assert!(
            preamble.starts_with(STANCE),
            "the stance opens the preamble: {preamble}"
        );
        let section = preamble
            .split_once("\n\nEnvironment\n")
            .map(|(_, section)| section)
            .unwrap_or_else(|| panic!("an Environment section follows the stance: {preamble}"));
        assert!(
            section.starts_with(&format!("  workspace: {WORKSPACE} ")),
            "and its one fact is the workspace line, holding the run's own path: {section}"
        );
        assert_eq!(
            section.lines().count(),
            1,
            "one line, and nothing else in the section: {section}"
        );
    }

    #[test]
    fn the_shipped_stance_holds_no_placeholder() {
        // A path typed into the shipped text is a path that can be wrong; the
        // only workspace the preamble carries is the one a run derives.
        for placeholder in ['{', '}', '<', '$'] {
            assert!(
                !STANCE.contains(placeholder),
                "the shipped text is a prompt, not a template: {placeholder:?} in {STANCE}"
            );
        }
    }

    #[test]
    fn the_stance_keeps_the_reading_rule_and_the_mermaid_fact() {
        // Every word here is re-sent on every turn of every run: the stance is
        // the two sentences measured to be worth their price, and no more.
        for kept in [READING_RULE, MERMAID] {
            assert!(
                STANCE.contains(kept),
                "the stance still carries `{kept}`: {STANCE}"
            );
        }
        // The ceiling rose from 80 when the many-inputs sentence landed: the
        // batching delta is the one thing the `bash` description cannot say, and
        // it costs its words like every other one here.
        let words = STANCE.split_whitespace().count();
        assert!(
            words <= 90,
            "the stance stays at or under 90 words, not {words}: {STANCE}"
        );
    }

    #[test]
    fn the_instructions_follow_the_preamble_whole_and_change_no_word_of_it() {
        let rules = "Be brief, and prove what you claim.\n";

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
    fn the_sandbox_states_the_boundary_and_the_preamble_leaves_it_alone() {
        // The policy's description carries the boundary; the preamble repeats
        // no part of it, because a boundary told twice is enforced once.
        let workspace = tempfile::TempDir::new().expect("workspace");
        let policy = crate::sandbox::SandboxPolicy::fixture(workspace.path());
        let description = policy.describe();
        let preamble = of(workspace.path(), None);
        let preamble = preamble.as_str();

        for fact in ["$TMPDIR", "HOME", "read-only", "network", "writable"] {
            assert!(
                description.contains(fact),
                "the tool description is where `{fact}` is said: {description}"
            );
            assert!(
                !preamble.contains(fact),
                "the preamble states no `{fact}` a second time: {preamble}"
            );
        }
        let temp = policy.session_temp().display().to_string();
        assert!(
            description.contains(&temp) && !preamble.contains(&temp),
            "the session temp is the description's alone: {description} vs {preamble}"
        );

        // The one string both texts hold is the workspace path, because the
        // writable set's first entry is the workspace the run stands in. It is not a
        // fact said twice: the preamble names where the run stands, the policy
        // names what may be written, and each would say something else about a
        // different path. Every other line of the description is the policy's
        // alone.
        let workspace_path = workspace.path().display().to_string();
        let shared: Vec<&str> = description
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && preamble.contains(line))
            .collect();
        assert_eq!(
            shared,
            vec![workspace_path.as_str()],
            "the writable set's entry is the whole of what the two texts share: {description}"
        );
    }

    #[test]
    fn the_batching_habit_is_the_delta_the_bash_description_does_not_say() {
        // The delta is read against the description the tool actually carries,
        // so a sentence the description gains or loses is felt here rather than
        // quoted from a copy that would drift.
        let workspace = tempfile::TempDir::new().expect("workspace");
        let policy = crate::sandbox::SandboxPolicy::fixture(workspace.path());
        let tool = crate::agent::BashTool::new(
            &policy,
            crate::agent::Limits::default(),
            std::sync::Arc::new(Silent),
        );
        let description = tool.spec().description.clone();
        let preamble = of(workspace.path(), None);
        let preamble = preamble.as_str();
        const MANY: &str = "Many inputs are one script";

        assert!(
            description.contains("put the steps of one task in one script"),
            "the procedure is the description's: {description}"
        );
        assert!(
            preamble.contains(MANY),
            "the many-inputs habit is the preamble's one sentence to add: {preamble}"
        );
        for sentence in description.split(". ") {
            let sentence = sentence.trim().trim_end_matches('.');
            assert!(
                !preamble.contains(sentence),
                "the preamble repeats no sentence of the description (`{sentence}`): {preamble}"
            );
        }
    }
}
