//! The preamble is the system message a run is sent with, re-sent every turn, so
//! every word in it is a tax: the persona, the principles, the environment the
//! run stands in, and the project's own instructions when its repository root
//! carries an `AGENTS.md`.
//!
//! It is a function of the workspace and of nothing else — not the catalog, not
//! the tool set, not the clock — so a build that offers no skill and a build that
//! offers several send one system message, byte for byte. That is the point of
//! putting the catalog in the tool descriptions instead: skills are paid for
//! where they exist, and absent skills cost the preamble nothing.
//!
//! Nothing that moves inside a run enters it. A preamble that changed between
//! turns would waste every cache breakpoint spent on it, so there is no
//! timestamp, no session id and no turn count here.

use std::path::{Path, PathBuf};

use crate::skills::repository_root;

/// The file a repository root may carry its own instructions in. §8 reads this
/// one and no nested one, because house rules arrive without being configured.
pub const PROJECT_INSTRUCTIONS_FILE: &str = "AGENTS.md";

/// Who is answering, in the voice the rest of the message is written in.
const PERSONA: &str = "\
## Persona

You are Sindri, an AI agent for GNU/Linux that acts through `bash` and answers the person in \
front of you in words they can use.";

/// The habits that shape an answer, kept to a handful: a long list of principles
/// is a list nobody follows.
const PRINCIPLES: &str = "\
## Principles

- One clean strike beats a flurry: batch what is independent, chain what depends on what.
- Read the smallest thing that tells the whole truth before you change anything.
- Change what the task needs, in the style already around it.
- When the stakes are high or you are unsure, name your weakest assumption rather than hedge.
- Report what you observed; a result you did not see is not a result.";

/// The preamble a run stands in front of: the workspace it works in, and the
/// project instructions its repository root carries, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preamble {
    /// The one directory the run stands in, which relative paths resolve against.
    workspace: PathBuf,
    /// The `AGENTS.md` at the repository root, when the root holds one.
    project_instructions: Option<String>,
}

impl Preamble {
    /// The preamble for a run standing in `workspace`: §9's three sections, and
    /// the repository root's `AGENTS.md` appended when there is one.
    ///
    /// A skill, a tool set or a turn count is deliberately not an argument: this
    /// value cannot vary with what the run happens to have discovered, which is
    /// what keeps the bytes identical across builds.
    pub fn for_workspace(workspace: &Path) -> Preamble {
        Preamble {
            workspace: workspace.to_path_buf(),
            project_instructions: project_instructions(&repository_root(workspace)),
        }
    }

    /// The bytes the system message carries: the sections in the order §9 asks
    /// for them, and the project's own words last, fenced so they read as the
    /// project speaking rather than as the agent's own habits.
    pub fn render(&self) -> String {
        let mut sections = vec![
            String::from(PERSONA),
            String::from(PRINCIPLES),
            environment(&self.workspace),
        ];
        if let Some(instructions) = &self.project_instructions {
            sections.push(format!(
                "<project_instructions>\n{instructions}\n</project_instructions>"
            ));
        }
        sections.join("\n\n")
    }
}

/// The environment section: the facts a run cannot read off the preamble any
/// other way — the platform, the shell, how a command is staged, where scratch
/// goes, and the one directory the run stands in.
fn environment(workspace: &Path) -> String {
    format!(
        "\
## Environment

- GNU/Linux, `bash` first: the shell is how a run acts.
- Stage multi-step commands under `set -o pipefail`, and label each stage so a failure names itself.
- Anything a run makes goes in `$TMPDIR`; never a hard-coded `/tmp`.
- The run stands in `{}`; relative paths resolve there.",
        workspace.display()
    )
}

/// The instructions a repository root carries, or `None` when it carries none:
/// absent means nothing is added, never an empty section.
///
/// A file that will not read is treated as absent here. §8 owns naming the file
/// and the reason to the person, and reporting the bytes given over; this
/// function owns only the bytes themselves.
fn project_instructions(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(PROJECT_INSTRUCTIONS_FILE)).ok()?;
    let text = text.trim_end();
    if text.is_empty() {
        return None;
    }
    Some(text.to_owned())
}
