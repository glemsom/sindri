//! The tools a run has: `bash` always, `skill` only when the workspace holds one
//! the model may see, and `open` only in a front end a human with a browser is
//! sitting in front of. One place takes a call to the tool that answers it, and
//! the match there is over [`Asked`] — a variant per tool — so a fourth tool is
//! a compile error rather than a tool that never runs.

use super::{BashTool, Loaded, OpenTool, SkillTool};
use crate::provider::{Asked, Call, ToolOutcome, ToolSpec};

pub struct Tools {
    bash: BashTool,
    skill: Option<SkillTool>,
    open: Option<OpenTool>,
}

impl Tools {
    /// The optional tools as options: a front end that has no catalog to offer
    /// and no browser to open hands `None`, and its schema never mentions
    /// either. Absence, not refusal — a refusal is a result the model pays for.
    pub fn new(bash: BashTool, skill: Option<SkillTool>, open: Option<OpenTool>) -> Self {
        // The pointer rides on `bash`, the schema a model reads while it is
        // deciding to run something. Whether there is one to make is decided
        // here, because this is the one place that knows whether `skill` is
        // registered at all.
        let bash = match skill {
            Some(_) => bash.pointing_at_skills(),
            None => bash,
        };
        Tools { bash, skill, open }
    }

    /// Every tool the model is offered this turn. With no skill it may see, the
    /// model is handed `bash` and nothing else: a tool with an empty catalog
    /// would charge every run for every run's silence.
    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut specs = vec![self.bash.spec().clone()];
        specs.extend(self.skill.as_ref().map(|skill| skill.spec().clone()));
        specs.extend(self.open.as_ref().map(|open| open.spec().clone()));
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
            // A run with no browser is answered the way `skill` answers a
            // workspace with nothing to load: the tool is absent, and the run
            // says so rather than pretending the call never happened.
            Asked::Open { target } => Some(match &self.open {
                Some(open) => open.call(call, &target),
                None => ToolOutcome::malformed(
                    call,
                    "there is no `open` tool in this run: this front end has no browser to \
                     open it in",
                ),
            }),
            Asked::Refused { problem, .. } => Some(ToolOutcome::malformed(call, &problem)),
        }
    }
}
