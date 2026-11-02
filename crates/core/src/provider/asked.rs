//! What one call asks for, once its arguments are read: the variant is the
//! tool. Every match over it — the loop's dispatch, the record's line, the
//! frame's row — therefore has to say what it does with a third tool, instead
//! of letting a call run that nothing answers or be written down as a shape no
//! reader knows.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{Call, json_kind};

/// The three tools a run may have, and the call that could not be read. `Refused`
/// is a variant rather than an error beside the value, so a call whose arguments
/// were unusable is still a call the loop answers, the record writes and the
/// run's budget pays for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum Asked {
    /// The shell: one script, run inside the sandbox.
    Bash { command: String },
    /// A skill by name, loaded into this conversation once.
    Skill { name: String },
    /// One target for the human's browser: an `http` or `https` URL, read out of
    /// the two argument shapes here, so a call that names none, names both or
    /// names a scheme this does not open is a [`Asked::Refused`] before any host
    /// is asked anything.
    Open { target: String },
    /// A call no tool can answer: a name this run does not have, or arguments
    /// that are not what the tool takes. `named` is the name as the model wrote
    /// it, because the call happened under it whatever it meant.
    Refused { named: String, problem: String },
}

const BASH: &str = "bash";
const SKILL: &str = "skill";
const OPEN: &str = "open";

/// What a `path` is answered with until the guard that decides which files may
/// be shown exists: the host's mimetype handler is the escalation, so a path no
/// rule has read is not a target.
const UNGUARDED: &str = "`open` does not open a `path`: nothing in this run decides which \
                         files may be shown to the human, so send an `http://` or `https://` \
                         `url`";

impl Call {
    /// The call as the loop reads it. Read once, here, so the tool that runs it,
    /// the line the record keeps and the row a front end draws cannot disagree
    /// about what was asked.
    pub fn asked(&self) -> Asked {
        let read = match self.name.as_str() {
            BASH => self
                .argument("command", r#"{"command": "<bash script>"}"#)
                .map(|command| Asked::Bash { command }),
            SKILL => self
                .argument("name", r#"{"name": "<a skill the catalog lists>"}"#)
                .map(|name| Asked::Skill { name }),
            OPEN => self.target().map(|target| Asked::Open { target }),
            named => Err(format!("there is no tool called `{named}` in this run")),
        };
        read.unwrap_or_else(|problem| Asked::Refused {
            named: self.name.clone(),
            problem,
        })
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

    /// The one target an `open` call names: exactly one of `url` and `path`, and
    /// a `url` is handed on only when its scheme is one a browser is asked for a
    /// page with.
    fn target(&self) -> Result<String, String> {
        let arguments = self
            .sent()
            .map_err(|sent| format!("the arguments were not a JSON object: `{sent}`"))?;
        let url = optional(arguments, "url")?;
        let path = optional(arguments, "path")?;
        match (url, path) {
            (Some(url), None) => page(url),
            (None, Some(_)) => Err(UNGUARDED.to_string()),
            (None, None) => Err(
                "the call names no target: send one of `{\"url\": \"<an http or https URL>\"}` \
                 and `{\"path\": \"<a file in the workspace>\"}`"
                    .to_string(),
            ),
            (Some(_), Some(_)) => Err(
                "the call names two targets: send one `url` or one `path`, never both".to_string(),
            ),
        }
    }
}

/// One optional argument: absent and empty are the same nothing, and a value of
/// the wrong type is a refusal rather than a string this parse invents.
fn optional(
    arguments: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match arguments.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value.clone())),
        Some(other) => Err(format!(
            "`{key}` must be a string, not {}",
            json_kind(other)
        )),
    }
}

/// A target the browser may be asked for: `http` or `https` and nothing else, so
/// a browser handed it has stayed where a run has a claim.
fn page(target: String) -> Result<String, String> {
    match scheme(&target) {
        Some(scheme)
            if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") =>
        {
            Ok(target)
        }
        Some(scheme) => Err(format!(
            "`open` does not open `{scheme}:` targets: it opens `http:` and `https:` URLs, and \
             `{scheme}:` is neither"
        )),
        None => Err(format!(
            "`{target}` names no scheme: `open` takes an `http://` or `https://` URL"
        )),
    }
}

/// The scheme a target opens with, if it opens with one: the text before the
/// first `:`, when it is a scheme's own alphabet and comes before any `/`.
fn scheme(target: &str) -> Option<&str> {
    let (head, _) = target.split_once(':')?;
    if head.contains('/') {
        return None;
    }
    let mut chars = head.chars();
    if !chars.next()?.is_ascii_alphabetic() {
        return None;
    }
    chars
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
        .then_some(head)
}

impl Asked {
    /// The tool this call named: `bash`, `skill`, `open`, or — for a call no tool
    /// could answer — the name it was made under.
    pub fn tool(&self) -> &str {
        match self {
            Asked::Bash { .. } => BASH,
            Asked::Skill { .. } => SKILL,
            Asked::Open { .. } => OPEN,
            Asked::Refused { named, .. } => named,
        }
    }
}

/// What the call asked for as one line: the script, the skill, the target, or
/// why nothing ran. The record and the frame read this rather than each writing
/// its own.
impl fmt::Display for Asked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Asked::Bash { command } => write!(f, "$ {command}"),
            Asked::Skill { name } => write!(f, "load {name}"),
            Asked::Open { target } => write!(f, "open {target}"),
            Asked::Refused { problem, .. } => write!(f, "not run: {problem}"),
        }
    }
}
