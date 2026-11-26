use std::path::{Path, PathBuf};

use crate::helpers::Helpers;
use crate::skills::repository_root;

pub const PROJECT_INSTRUCTIONS_FILE: &str = "AGENTS.md";

const PERSONA: &str = "\
## Persona

You are Sindri, an AI assistant.";

const PRINCIPLES: &str = "\
## Principles

- Batch what is independent; chain what depends on what.
- Locate relevant files before reading their contents, unless the target is already known.
- Read the smallest scope that gives sufficient context; widen it when needed. Do not treat partial output as complete evidence.
- Make the smallest change that satisfies the task, in the surrounding style, preserving unrelated content and existing user changes.
- When the stakes are high or you are unsure, name your weakest assumption rather than hedge.
- Report what you observed; a result you did not see is not a result.
- Say what you found, not what pleases: no flattery, no forced enthusiasm.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preamble {
    workspace: PathBuf,
    project_instructions: Option<String>,
    helpers: Option<Helpers>,
}

impl Preamble {
    pub fn for_workspace(workspace: &Path) -> Preamble {
        Preamble {
            workspace: workspace.to_path_buf(),
            helpers: None,
            project_instructions: project_instructions(&repository_root(workspace)),
        }
    }

    pub fn helpers(mut self, helpers: Helpers) -> Self {
        self.helpers = Some(helpers);
        self
    }

    pub fn render(&self) -> String {
        let mut sections = vec![
            String::from(PERSONA),
            String::from(PRINCIPLES),
            environment(&self.workspace, self.helpers.as_ref()),
        ];
        if let Some(instructions) = &self.project_instructions {
            sections.push(format!(
                "<project_instructions>\n{instructions}\n</project_instructions>"
            ));
        }
        sections.join("\n\n")
    }
}

fn environment(workspace: &Path, helpers: Option<&Helpers>) -> String {
    let mut rendered = format!(
        "\
## Environment

- GNU/Linux; use `bash` for shell commands.
- For multi-stage commands, use `set -o pipefail` and label each stage.
- Put temporary artifacts in `$TMPDIR`; never hard-code `/tmp`.
- The working directory is `{}`; relative paths resolve there.",
        workspace.display()
    );
    if let Some(helpers) = helpers {
        rendered.push_str("\n");
        rendered.push_str(&helpers.render());
    }
    rendered.push_str("\n- Helper availability is a launch-time snapshot, not an exhaustive inventory; later command results take precedence. Check unlisted commands and required versions before relying on them.\n- Prefer Python’s standard library for ordinary file work; check third-party dependencies in the task’s intended interpreter or project environment.");
    rendered
}

fn project_instructions(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(PROJECT_INSTRUCTIONS_FILE)).ok()?;
    let text = text.trim_end();
    if text.is_empty() {
        return None;
    }
    Some(text.to_owned())
}
