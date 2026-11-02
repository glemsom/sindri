//! The `open` tool: one target, handed to the host's opener, and a result that
//! says the host was asked rather than that a human looked. The core decides and
//! refuses; the spawn is the [`Opener`]'s, because the display, the session bus
//! and the browser belong to the process the run was started from and the
//! sandbox is the thing that takes them away.

use std::sync::Arc;
use std::time::Instant;

use crate::provider::{Call, ToolFacts, ToolOutcome, ToolSpec};

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
}

impl OpenTool {
    pub fn new(opener: Arc<dyn Opener>) -> OpenTool {
        OpenTool {
            spec: describe(),
            opener,
        }
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    /// One call. The target is the one the parse already read out of the
    /// arguments, so a call that named none, both, or a scheme this does not
    /// open never reaches here and never asks the host anything.
    pub fn call(&self, call: &Call, target: &str) -> ToolOutcome {
        let started = Instant::now();
        let (content, outcome, failed) = match self.opener.open(target) {
            Opened::Started => (
                format!("outcome: started\ntarget: {target}\n"),
                "started",
                false,
            ),
            Opened::Failed { reason } => (
                format!("outcome: failed\ntarget: {target}\nproblem: {reason}\n"),
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
}

/// The doctrine and the two argument shapes, in the one place a model reads
/// while it is choosing a tool. It is paid for only where the tool is
/// registered, which is why it says nothing about a run that has no browser.
fn describe() -> ToolSpec {
    let description = "Puts one thing in front of the human's eyes, in the browser of the machine \
         running sindri: for them, not for you. It is how you show someone a page you wrote or \
         found, and never how you read one — `bash` is how you read.\n\
         One target per call, named one way: `url`, an `http://` or `https://` address the host's \
         browser opens; `path`, a file in the workspace, which is not opened yet because nothing \
         here decides which files are safe to show.\n\
         The result is `started` — the host was asked and had not refused it a quarter of a \
         second later — or `failed:` with the reason the opener gave. Neither means a human \
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
                    "description": "An `http://` or `https://` address, opened in the human's browser.",
                },
                "path": {
                    "type": "string",
                    "description": "A file in the workspace to show them. Not opened yet.",
                },
            },
        }),
    }
}

fn millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
