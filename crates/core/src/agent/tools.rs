//! The tools a run has: `bash` always, and `skill` only when the workspace
//! holds one the model may see. One place takes a call to the tool that answers
//! it, and the match there is over [`Asked`] — a variant per tool — so a third
//! tool is a compile error rather than a tool that never runs.

use super::{BashTool, Loaded, SkillTool};
use crate::provider::{Asked, Call, ToolOutcome, ToolSpec};

pub struct Tools {
    bash: BashTool,
    skill: Option<SkillTool>,
}

impl Tools {
    pub fn new(bash: BashTool, skill: Option<SkillTool>) -> Self {
        Tools { bash, skill }
    }

    /// Every tool the model is offered this turn. With no skill it may see, the
    /// model is handed `bash` and nothing else: a tool with an empty catalog
    /// would charge every run for every run's silence.
    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut specs = vec![self.bash.spec().clone()];
        specs.extend(self.skill.as_ref().map(|skill| skill.spec().clone()));
        specs
    }

    /// One call, run. `None` is a call a cancel killed mid-flight, which the
    /// loop reads as the run ending.
    pub fn call(&self, call: &Call, loaded: &Loaded) -> Option<ToolOutcome> {
        match call.asked() {
            Asked::Bash { command } => self.bash.call(call, &command),
            // A model can name a tool it was not handed; the run answers rather
            // than pretending the call never happened.
            Asked::Skill { name } => Some(match &self.skill {
                Some(skill) => skill.call(call, &name, loaded),
                None => ToolOutcome::malformed(
                    call,
                    "there is no `skill` tool in this run: this workspace holds no skill the \
                     model may see",
                ),
            }),
            Asked::Refused { problem, .. } => Some(ToolOutcome::malformed(call, &problem)),
        }
    }
}
