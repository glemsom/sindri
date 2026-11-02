//! The `open` tool: one target, handed to the host's opener, and a result that
//! says the host was asked rather than that a human looked. The core decides and
//! refuses; the spawn is the [`Opener`]'s, because the display, the session bus
//! and the browser belong to the process the run was started from and the
//! sandbox is the thing that takes them away.
//!
//! A `path` — written as `path`, or as the `file:` URI that is the same thing
//! spelled differently — is guarded here, against the policy's own sets, because
//! showing a file is only ever what a run could already have written: the host
//! gives the file to whichever handler its desktop registered, so the guard is
//! the whole of what keeps a write permission from becoming a way to run
//! something with the human's privileges. It is resolved first and the resolved path is what is
//! handed over, so a link cannot be a promise the tool does not keep.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::provider::{Call, Target, ToolFacts, ToolOutcome, ToolSpec, reads};
use crate::sandbox::SandboxPolicy;

/// What came of asking the host for one target. There is no third value: whether
/// a window drew, whether the session had a display and whether anybody turned
/// around are on the far side of a process boundary, and a result that guessed
/// at them would be a success nobody saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// The opener took the target and had not refused it when the probe window
    /// closed — it is still running, or it exited without a complaint.
    Started,
    /// It started nothing, in the opener's own words.
    Failed { reason: String },
}

/// The last step, and the one the core cannot take: the host spawns the opener,
/// outside the sandbox, with the environment only it holds.
pub trait Opener: Send + Sync {
    fn open(&self, target: &str) -> Opened;
}

pub struct OpenTool {
    spec: ToolSpec,
    opener: Arc<dyn Opener>,
    /// The tree a relative `path` is read from: the directory a command starts
    /// in inside the sandbox, so a path means what it meant to the model.
    workspace: PathBuf,
    /// The sets a run may write, taken from the policy rather than written down
    /// again: the guard admits exactly what the sandbox would let a command put
    /// there.
    policy: SandboxPolicy,
}

impl OpenTool {
    pub fn new(opener: Arc<dyn Opener>, policy: SandboxPolicy, workspace: PathBuf) -> OpenTool {
        OpenTool {
            spec: describe(),
            opener,
            workspace,
            policy,
        }
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    /// One call. The target is the one the parse already read out of the
    /// arguments, so a call that named none, both, or a target the host may not
    /// be given never reaches here and never asks the host anything.
    pub fn call(&self, call: &Call, target: &Target) -> ToolOutcome {
        let started = Instant::now();
        // A path the guard admitted is handed over as the guard resolved it, so a
        // model that wrote a relative one can quote the path a human could paste.
        let shown = match target {
            Target::Url(url) => url.clone(),
            Target::Path(given) => match self.guarded(given) {
                Ok(path) => path.display().to_string(),
                Err(problem) => return ToolOutcome::malformed(call, &problem),
            },
        };
        let (content, outcome, failed) = match self.opener.open(&shown) {
            Opened::Started => (
                format!("outcome: started\ntarget: {shown}\n"),
                "started",
                false,
            ),
            Opened::Failed { reason } => (
                format!("outcome: failed\ntarget: {shown}\nproblem: {reason}\n"),
                "failed",
                true,
            ),
        };
        ToolOutcome::new(call, content, failed).with_facts(ToolFacts {
            outcome: Some(outcome.to_string()),
            duration_ms: Some(millis(started)),
            ..ToolFacts::default()
        })
    }

    /// The file a `path` names, as the host is handed it: absolute, resolved,
    /// inside what a run may write, a regular file, and of a kind a browser
    /// reads. Both the path as given and the path it resolves to are held to
    /// the sets, and the kind is read on the resolved one, because a link
    /// inside the workspace that points out of it is a write permission
    /// laundered into a thing shown to a human — and the resolved path is what
    /// is handed over, so a check on the string the model wrote would be a
    /// check on the wrong string.
    fn guarded(&self, given: &str) -> Result<PathBuf, String> {
        let as_given = self.workspace.join(given);
        if !self.policy.may_write(&as_given) {
            return Err(format!(
                "`{given}` is outside the writable set and the session temp: `open` shows a \
                 human what a run could have written, and this run may write {}",
                self.writable()
            ));
        }
        let resolved = as_given
            .canonicalize()
            .map_err(|source| format!("`{given}` does not exist, or cannot be read: {source}"))?;
        if !self.policy.may_write(&resolved) {
            // Where a link led is not said: the path is outside what a run may
            // read, so naming it would make the refusal a way to ask the host
            // whether a file the run cannot read exists.
            return Err(format!(
                "`{given}` is inside the writable set but resolves to a file outside it: a link \
                 may not turn a write permission into a file shown to a human"
            ));
        }
        if !resolved.is_file() {
            return Err(format!(
                "`{}` is not a regular file: the host hands a file to a handler, and a browser is \
                 never given a directory or a device",
                resolved.display()
            ));
        }
        // The kind is read again, on the resolved file: a link named `.html` may
        // be a `.desktop` underneath, and the resolved path is what is handed
        // over, so a check on the spelling alone would be a check on the wrong
        // name.
        reads(&resolved)?;
        Ok(resolved)
    }

    /// The places a `path` may lie in, as a refusal names them: the policy's own
    /// writable set and its session temp, never a list written here.
    fn writable(&self) -> String {
        let mut places: Vec<String> = self
            .policy
            .writable()
            .paths()
            .iter()
            .map(|path| format!("`{}`", path.display()))
            .collect();
        places.push(format!(
            "the session temp `{}`",
            self.policy.session_temp().display()
        ));
        places.join(", ")
    }
}

/// The doctrine and the two argument shapes, in the one place a model reads
/// while it is choosing a tool. It is paid for only where the tool is
/// registered, which is why it says nothing about a run that has no browser.
fn describe() -> ToolSpec {
    let description = "Puts one thing in front of the human's eyes, in the browser of the machine \
         running sindri: for them, not for you. It is how you show someone a page you wrote or \
         found, and never how you read one — `bash` is how you read.\n\
         One target per call, named one way: `url`, an `http://` or `https://` address the host's \
         browser fetches, or a `file:` URI, which is `path` written another way; `path`, a file a \
         run could have written — inside the writable set or the session temp, and of a kind a \
         browser reads, since the host hands a file to whatever handler its desktop registered. A \
         refusal names the whole list of kinds.\n\
         A `file:` URI is a second spelling of `path` and not a second door: `url` may carry one, \
         and it is percent-decoded and reduced to the path it names — an empty authority or \
         `localhost`, never another host — before the same rule reads it, so it opens nothing a \
         `path` could not and is refused in the same words.\n\
         The result is `started` — the host was asked and had not refused it a quarter of a \
         second later — or `failed:` with the reason the opener gave. A `path` is echoed as the \
         absolute, resolved path the host was given, not as you typed it. Neither means a human \
         looked: the window is on the far side of a process boundary."
        .to_string();
    ToolSpec {
        name: "open".to_string(),
        description,
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "An `http://` or `https://` address, opened in the human's \
                                    browser, or a `file:` URI, which is a `path` in another \
                                    spelling and passes the same rules.",
                },
                "path": {
                    "type": "string",
                    "description": "A file to show them: inside the writable set or the session \
                                    temp, and of a kind a browser reads.",
                },
            },
        }),
    }
}

fn millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
