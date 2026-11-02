//! What one call asks for, once its arguments are read: the variant is the
//! tool. Every match over it — the loop's dispatch, the record's line, the
//! frame's row — therefore has to say what it does with a third tool, instead
//! of letting a call run that nothing answers or be written down as a shape no
//! reader knows.

use std::ffi::OsStr;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
    /// One target for the human's browser, read out of the two argument shapes
    /// here, so a call that names none, names both, or names a target the host
    /// may not be handed is a [`Asked::Refused`] before anything is opened.
    Open { target: Target },
    /// A call no tool can answer: a name this run does not have, or arguments
    /// that are not what the tool takes. `named` is the name as the model wrote
    /// it, because the call happened under it whatever it meant.
    Refused { named: String, problem: String },
}

const BASH: &str = "bash";
const SKILL: &str = "skill";
const OPEN: &str = "open";

/// What one `open` call names, and the reason it is a value rather than the
/// string the model wrote: a page and a file are read by different rules — a
/// scheme for one, a guard over the filesystem for the other — so a target that
/// had forgotten which it was could only be guessed at by a second reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// An `http` or `https` address the host's browser fetches itself.
    Url(String),
    /// A file the host hands to the browser, as the model wrote it: the guard
    /// that resolves and admits it is the tool's, because it reads the policy.
    Path(String),
}

impl Target {
    pub fn as_str(&self) -> &str {
        match self {
            Target::Url(text) | Target::Path(text) => text,
        }
    }

    /// The same target with its text passed through `f`: a record redacts what
    /// a target holds as it redacts a command, whichever shape it is.
    pub fn map(self, f: impl FnOnce(String) -> String) -> Target {
        match self {
            Target::Url(text) => Target::Url(f(text)),
            Target::Path(text) => Target::Path(f(text)),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Target, D::Error> {
        let text = String::deserialize(deserializer)?;
        // A record keeps a target as one string, and nothing that reads a record
        // opens anything: a target read back is classified by its own text,
        // which is the only thing there is to classify it by.
        Ok(match scheme(&text) {
            Some(scheme) if web(scheme) => Target::Url(text),
            _ => Target::Path(text),
        })
    }
}

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
    /// each is read by the rule that holds for it — a `url` by its scheme, a
    /// `path` by the extensions a browser reads — so the target handed on is one
    /// the host may be given as what it is.
    fn target(&self) -> Result<Target, String> {
        let arguments = self
            .sent()
            .map_err(|sent| format!("the arguments were not a JSON object: `{sent}`"))?;
        let url = optional(arguments, "url")?;
        let path = optional(arguments, "path")?;
        match (url, path) {
            (Some(url), None) => page(url).map(Target::Url),
            (None, Some(path)) => file(path).map(Target::Path),
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
        Some(scheme) if web(scheme) => Ok(target),
        Some(scheme) => Err(format!(
            "`open` does not open `{scheme}:` targets: it opens `http:` and `https:` URLs, and \
             `{scheme}:` is neither"
        )),
        None => Err(format!(
            "`{target}` names no scheme: `open` takes an `http://` or `https://` URL"
        )),
    }
}

/// Whether a scheme names a page a browser is asked for.
fn web(scheme: &str) -> bool {
    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
}

/// The extensions the host's handler may be trusted with. One list in one place:
/// a refusal names the whole of it, and nothing else repeats it.
const READABLE: [&str; 12] = [
    "html", "htm", "pdf", "png", "jpg", "jpeg", "gif", "webp", "svg", "txt", "md", "csv",
];

/// A file the host may be asked to show. The handler is the escalation and not
/// the scheme — the host hands a file to whatever its desktop registered, so a
/// `.desktop` or a script would run with the human's privileges — and the
/// extension in the spelling the model wrote is read here, before anything
/// resolves, so a call naming a kind no browser reads never reaches the guard.
fn file(given: String) -> Result<String, String> {
    reads(Path::new(&given))?;
    Ok(given)
}

/// Whether `path` ends in a kind a browser reads, and when it does not, the
/// refusal, naming the whole list. One rule read in two places because the two
/// strings differ: the model's, here, and the resolved path's in the guard —
/// a link inside the writable set named `report.html` resolves to a
/// `payload.desktop`, and the resolved file is the one the host is handed.
pub fn reads(path: &Path) -> Result<(), String> {
    let extension = path.extension().and_then(OsStr::to_str);
    if let Some(extension) = extension
        && READABLE
            .iter()
            .any(|read| extension.eq_ignore_ascii_case(read))
    {
        return Ok(());
    }
    let name = path.display();
    let why = match extension {
        Some(extension) => format!("ends in `.{extension}`"),
        None => "has no extension".to_string(),
    };
    Err(format!(
        "`open` does not open `{name}`: the host hands a file to whichever handler its desktop \
         registered, so a `path` must be one a browser reads — {} — and it {why}",
        readable()
    ))
}

/// The allow-list as a refusal names it: `html, htm, … csv`, each as an extension.
fn readable() -> String {
    READABLE.map(|extension| format!(".{extension}")).join(", ")
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
