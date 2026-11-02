//! What one call asks for, once its arguments are read: the variant is the
//! tool. Every match over it — the loop's dispatch, the record's line, the
//! frame's row — therefore has to say what it does with a third tool, instead
//! of letting a call run that nothing answers or be written down as a shape no
//! reader knows.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{Call, json_kind};

/// The two tools a run may have, and the call that could not be read. `Refused`
/// is a variant rather than an error beside the value, so a call whose
/// arguments were unusable is still a call the loop answers, the record writes
/// and the run's budget pays for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum Asked {
    /// The shell: one script, run inside the sandbox.
    Bash { command: String },
    /// A skill by name, loaded into this conversation once.
    Skill { name: String },
    /// A call no tool can answer: a name this run does not have, or arguments
    /// that are not what the tool takes. `named` is the name as the model wrote
    /// it, because the call happened under it whatever it meant.
    Refused { named: String, problem: String },
}

const BASH: &str = "bash";
const SKILL: &str = "skill";

impl Call {
    /// The call as the loop reads it. Read once, here, so the tool that runs it,
    /// the line the record keeps and the row a front end draws cannot disagree
    /// about what was asked.
    pub fn asked(&self) -> Asked {
        let (key, hint, build): (_, _, fn(String) -> Asked) = match self.name.as_str() {
            BASH => ("command", r#"{"command": "<bash script>"}"#, |command| {
                Asked::Bash { command }
            }),
            SKILL => (
                "name",
                r#"{"name": "<a skill the catalog lists>"}"#,
                |name| Asked::Skill { name },
            ),
            named => {
                return Asked::Refused {
                    named: named.to_string(),
                    problem: format!("there is no tool called `{named}` in this run"),
                };
            }
        };
        match self.argument(key, hint) {
            Ok(value) => build(value),
            Err(problem) => Asked::Refused {
                named: self.name.clone(),
                problem,
            },
        }
    }

    /// One argument as the string the tool takes: missing, empty and the wrong
    /// type are all answered — with what to send instead — because a model that
    /// hears why fixes the call on the next turn.
    fn argument(&self, key: &str, hint: &str) -> Result<String, String> {
        let arguments = self
            .sent()
            .map_err(|sent| format!("the arguments were not a JSON object: `{sent}`"))?;
        match arguments.get(key) {
            Some(serde_json::Value::String(value)) if !value.trim().is_empty() => Ok(value.clone()),
            Some(serde_json::Value::String(_)) | None => {
                Err(format!("the call has no `{key}`: send {hint}"))
            }
            Some(other) => Err(format!(
                "`{key}` must be a string, not {}",
                json_kind(other)
            )),
        }
    }
}

impl Asked {
    /// The tool this call named: `bash`, `skill`, or — for a call no tool could
    /// answer — the name it was made under.
    pub fn tool(&self) -> &str {
        match self {
            Asked::Bash { .. } => BASH,
            Asked::Skill { .. } => SKILL,
            Asked::Refused { named, .. } => named,
        }
    }
}

/// What the call asked for as one line: the script, the skill, or why nothing
/// ran. The record and the frame read this rather than each writing its own.
impl fmt::Display for Asked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Asked::Bash { command } => write!(f, "$ {command}"),
            Asked::Skill { name } => write!(f, "load {name}"),
            Asked::Refused { problem, .. } => write!(f, "not run: {problem}"),
        }
    }
}
