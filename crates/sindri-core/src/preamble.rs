use std::path::{Path, PathBuf};

use crate::skills::repository_root;

pub const PROJECT_INSTRUCTIONS_FILE: &str = "AGENTS.md";

const PERSONA: &str = "\
## Persona

You are Sindri, an AI assistant.";

const PRINCIPLES: &str = "\
## Principles

- Batch what is independent; chain what depends on what.
- Read the smallest thing that tells the whole truth before you change anything.
- Change what the task needs, in the style already around it.
- When the stakes are high or you are unsure, name your weakest assumption rather than hedge.
- Report what you observed; a result you did not see is not a result.
- Say what you found, not what pleases: no flattery, no forced enthusiasm.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preamble {
    workspace: PathBuf,
    project_instructions: Option<String>,
}

impl Preamble {
    pub fn for_workspace(workspace: &Path) -> Preamble {
        Preamble {
            workspace: workspace.to_path_buf(),
            project_instructions: project_instructions(&repository_root(workspace)),
        }
    }

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

fn environment(workspace: &Path) -> String {
    format!(
        "\
## Environment

- GNU/Linux; use `bash` for shell commands.
- For multi-stage commands, use `set -o pipefail` and label each stage.
- Put temporary artifacts in `$TMPDIR`; never hard-code `/tmp`.
- The working directory is `{}`; relative paths resolve there.",
        workspace.display()
    )
}

fn project_instructions(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(PROJECT_INSTRUCTIONS_FILE)).ok()?;
    let text = text.trim_end();
    if text.is_empty() {
        return None;
    }
    Some(text.to_owned())
}
