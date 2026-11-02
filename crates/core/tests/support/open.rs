//! The third tool's fixtures, for the two files that prove its contract: a
//! faked host opener that records every target it was asked for, and a run whose
//! workspace a test can put a file in. Nothing here reaches a browser or a
//! sandbox.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sindri_core::agent::{BashTool, Limits, OpenTool, Opened, Opener, Tools};
use sindri_core::provider::{Call, ToolOutcome};
use sindri_core::sandbox::SandboxPolicy;
use tempfile::TempDir;

use super::{Scripted, fixtures, policy};

/// The host's opener, faked: every target it was asked for, and the one answer
/// it gives. A test that says it was never asked reads an empty list.
pub struct Fake {
    seen: Mutex<Vec<String>>,
    answer: Opened,
}

impl Fake {
    pub fn answering(answer: Opened) -> Arc<Fake> {
        Arc::new(Fake {
            seen: Mutex::default(),
            answer,
        })
    }

    pub fn asked(&self) -> Vec<String> {
        self.seen.lock().expect("the opener's log").clone()
    }
}

impl Opener for Fake {
    fn open(&self, target: &str) -> Opened {
        self.seen
            .lock()
            .expect("the opener's log")
            .push(target.to_string());
        self.answer.clone()
    }
}

/// A run with the third tool, over an opener a test controls.
pub fn tools(opener: Arc<Fake>) -> Tools {
    let (workspace, _session_temp, policy) = fixtures();
    let root = workspace
        .path()
        .canonicalize()
        .expect("a canonical workspace");
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    Tools::new(bash, None, Some(OpenTool::new(opener, policy, root)))
}

/// A run with no browser anywhere near it, as batch mode builds one.
pub fn windowless() -> Tools {
    let (_workspace, _session_temp, policy) = fixtures();
    let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
    Tools::new(bash, None, None)
}

/// The call the model makes to `open`, with the arguments it sent.
pub fn call(arguments: &[(&str, &str)]) -> Call {
    let sent = arguments
        .iter()
        .map(|(key, value)| ((*key).to_string(), serde_json::json!(value)));
    Call::new("call_1", "open", Ok(serde_json::Map::from_iter(sent)))
}

pub fn open(target: &str) -> Call {
    call(&[("url", target)])
}

/// A run whose workspace a test can put a file in, and whose opener it reads
/// afterwards: the two places a `path` may name, and the third tool over them.
pub struct Run {
    pub workspace: TempDir,
    /// Held only where the session temp is a tree of its own: the shared
    /// fixtures keep it inside the workspace.
    _session: Option<TempDir>,
    pub session_temp: PathBuf,
    opener: Arc<Fake>,
    tools: Tools,
}

impl Run {
    pub fn new() -> Run {
        let (workspace, session_temp, policy) = fixtures();
        Run::over(workspace, None, session_temp, policy)
    }

    /// A run whose session temp is outside its workspace, so the two places a
    /// `path` may name are two different trees.
    pub fn apart() -> Run {
        let workspace = TempDir::new().expect("workspace");
        let session = TempDir::new().expect("session temp");
        let session_temp = session.path().to_path_buf();
        let policy = policy(&workspace, &session_temp);
        Run::over(workspace, Some(session), session_temp, policy)
    }

    fn over(
        workspace: TempDir,
        session: Option<TempDir>,
        session_temp: PathBuf,
        policy: SandboxPolicy,
    ) -> Run {
        let opener = Fake::answering(Opened::Started);
        let root = workspace
            .path()
            .canonicalize()
            .expect("a canonical workspace");
        let bash = BashTool::new(&policy, Limits::default(), Scripted::silent());
        let hand: Arc<dyn Opener> = opener.clone();
        let tools = Tools::new(bash, None, Some(OpenTool::new(hand, policy, root)));
        Run {
            workspace,
            _session: session,
            session_temp,
            opener,
            tools,
        }
    }

    /// A report written at `base`/`name`, as the canonical path the guard is
    /// expected to hand over.
    pub fn write(&self, base: &Path, name: &str) -> PathBuf {
        let path = base.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a directory for the report");
        }
        std::fs::write(&path, "<html>a report</html>").expect("a report");
        path.canonicalize().expect("a canonical report")
    }

    /// A report in the workspace.
    pub fn report(&self, name: &str) -> PathBuf {
        self.write(self.workspace.path(), name)
    }

    /// One `open` call naming a `path`, answered.
    pub fn shown(&self, given: &str) -> ToolOutcome {
        self.tools
            .call(&call(&[("path", given)]), &Default::default())
            .expect("a call is answered")
    }

    /// One `open` call naming a `url`, answered: the argument a `file:` URI is
    /// written in, and the one an `http` page is written in.
    pub fn via_url(&self, given: &str) -> ToolOutcome {
        self.tools
            .call(&call(&[("url", given)]), &Default::default())
            .expect("a call is answered")
    }

    pub fn asked(&self) -> Vec<String> {
        self.opener.asked()
    }
}

/// The whole allow-list, as a refusal names it: written out here rather than
/// read from the code, so the two cannot agree by construction.
pub const READABLE: &str =
    ".html, .htm, .pdf, .png, .jpg, .jpeg, .gif, .webp, .svg, .txt, .md, .csv";
