//! A run's stack, composed once: the cancel flag a handler raises, the executor
//! over the sandbox, `bash` with the limits its own description states, the tool
//! set, the dials the turn loop is wired with, and the channel its events travel
//! on. A front end hands this module the two things that differ between front
//! ends — how commands are dressed for its human, and whether there is a browser
//! to open a target in — and gets back an agent it can hand a prompt.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use sindri_core::agent::{Agent, BashTool, Dials, Event, Limits, OpenTool, Opener, Tools};
use sindri_core::cancel::Cancel;
use sindri_core::config::Config;
use sindri_core::exec::{BashExecutor, SandboxExecutor};
use sindri_core::provider::{Client, Error, Model};
use sindri_core::session::Session;

/// How a front end dresses the executor the session builds: what a command's
/// output is told to while it runs, and any wrapper that watches each result as
/// it comes back. The stack builds the executor and `bash` on it; the front end
/// says only what those look like from where its human is sitting.
pub trait Dress {
    fn dress(&self, executor: SandboxExecutor) -> Arc<dyn BashExecutor>;
}

/// A front end that watches nothing: the executor exactly as the session built
/// it, with nothing told of a command's output as it arrives.
pub struct Bare;

impl Dress for Bare {
    fn dress(&self, executor: SandboxExecutor) -> Arc<dyn BashExecutor> {
        Arc::new(executor)
    }
}

/// The names the wire a run speaks serves: asked of the model a prompt asks —
/// the same client, composed the same way — so the list a human is shown and the
/// model a run sends to cannot be two answers. Nothing of a turn is composed for
/// it: a listing is not a turn, and a header value carrying `{session}` is the
/// run's id on a turn and left off a listing (`provider::rig_wire::http`).
pub fn models(session: &Session, config: &Config) -> Result<Vec<String>, Error> {
    model(session, config).models()
}

/// The model one prompt asks: the client the wire is spoken through, built from
/// the run's config, the preamble the session settled and the id of the session
/// the run writes — the three facts a request is composed from.
fn model(session: &Session, config: &Config) -> Box<dyn Model> {
    Box::new(Client::new(config, session.preamble(), session.dir().id()))
}

/// Everything that differs between one front end's stack and the next, and
/// nothing that does not. A run is composed from this and never assembled by
/// hand: the parts are here because a front end owns them, not because the order
/// they are wired in is a front end's to decide.
pub struct Stack<'a> {
    pub session: &'a Session,
    pub config: &'a Config,
    /// The flag Ctrl-C raises: the loop stops on it between turns, and the
    /// executor kills a running command's group on it.
    pub cancel: Cancel,
    pub executor: &'a dyn Dress,
    /// The opener, where a human with a browser is in front of the run. Absent,
    /// the run is offered no `open` at all — an absence, not a refusal.
    pub opener: Option<Arc<dyn Opener>>,
}

impl Stack<'_> {
    /// The agent a front end hands one prompt, and the channel the events of that
    /// prompt travel on. The flag, the record and the skills are the session's,
    /// and the channel is closed by the caller when the run is over.
    pub fn agent(&self) -> (Agent, Receiver<Event>) {
        let mut agent = self.session.agent(self.dials(), self.cancel.clone());
        let events = self.wire(&mut agent);
        (agent, events)
    }

    /// The next prompt on the agent a front end already keeps, which is what a
    /// thread is: the dials composed here again — a saved setting changes what a
    /// prompt runs on and not what it is about — the launch's flag, put down
    /// before the prompt rather than kept from the last one, this session taken
    /// again, and a channel of its own, because a run's events are one run's.
    pub fn wire(&self, agent: &mut Agent) -> Receiver<Event> {
        agent.set_dials(self.dials());
        agent.set_cancel(self.cancel.clone());
        // `/new` puts a new record under the thread, so the kept agent takes the
        // session again rather than writing to the one it replaced.
        self.session.adopt(agent);
        let (sender, events) = mpsc::channel();
        agent.set_events(sender);
        events
    }

    /// One prompt's dials, in the one place they are composed: a dial added here
    /// is an edit here and nowhere else.
    fn dials(&self) -> Dials {
        // The description names the timeout this run enforces, so the model is
        // never told a boundary it does not have.
        let executor = self
            .session
            .executor(self.config.timeout, self.cancel.clone());
        let bash = BashTool::new(
            self.session.policy(),
            Limits::for_timeout(self.config.timeout),
            self.executor.dress(executor),
        );
        Dials {
            // The preamble is composed from the session rather than handed in:
            // the workspace a request names is the one the policy mounted.
            model: model(self.session, self.config),
            // The tool set decides whether `bash` carries the pointer to the
            // catalog, because it is the one place that knows whether `skill` is
            // registered at all.
            tool: Tools::new(bash, self.session.skill_tool(), self.open()),
            max_turns: self.config.max_turns,
            thinking: self.config.thinking,
        }
    }

    /// The third tool, where this front end has a browser to hand a target to.
    fn open(&self) -> Option<OpenTool> {
        self.opener.as_ref().map(|opener| {
            OpenTool::new(
                opener.clone(),
                self.session.policy().clone(),
                self.session.workspace().to_path_buf(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        reason = "a panicking test is how a broken seam is reported"
    )]

    use std::time::Duration;

    use sindri_core::agent::{Opened, Opener};
    use sindri_core::config::{Config, Provider};
    use sindri_core::sandbox::{Enforcement, Extras};
    use sindri_core::session::{Env, Session};
    use tempfile::TempDir;

    use super::*;

    struct NoBrowser;

    impl Opener for NoBrowser {
        fn open(&self, _target: &str) -> Opened {
            Opened::Started
        }
    }

    fn config(timeout: Duration) -> Config {
        Config {
            provider: Provider::Anthropic,
            model: "model-one".to_string(),
            api_key: String::new(),
            base_url: None,
            max_turns: 3,
            timeout,
            thinking: false,
            status: true,
            ssh_agent: true,
            extras: Extras::Named(Vec::new()),
            enforcement: Enforcement::Bubblewrap,
            extra_headers: Vec::new(),
            refused_repo_config: None,
        }
    }

    fn host(root: &TempDir) -> (Config, Session) {
        let workspace = root.path().join("workspace");
        let state = root.path().join("state");
        for dir in [&workspace, &state] {
            std::fs::create_dir_all(dir).expect("a temporary directory");
        }
        let env = Env {
            workspace: workspace.canonicalize().expect("canonical workspace"),
            search_path: "/usr/bin:/bin".to_string(),
            state,
            ssh_agent: None,
        };
        let config = config(Duration::from_secs(5));
        let session = Session::open(&env, &config).expect("a session in the environment");
        (config, session)
    }

    fn names(stack: &Stack<'_>) -> Vec<String> {
        stack
            .dials()
            .tool
            .specs()
            .iter()
            .map(|spec| spec.name.clone())
            .collect()
    }

    #[test]
    fn a_front_end_with_no_browser_is_offered_bash_and_skill_and_no_open_at_all() {
        let root = TempDir::new().expect("temp root");
        let (config, session) = host(&root);
        let bare = Bare;
        let stack = Stack {
            session: &session,
            config: &config,
            cancel: Cancel::new(),
            executor: &bare,
            opener: None,
        };

        assert_eq!(
            names(&stack),
            ["bash", "skill"],
            "a script has no window and nobody to look at it, so `open` is absent"
        );
    }

    #[test]
    fn a_front_end_with_a_browser_is_offered_open_beside_them() {
        let root = TempDir::new().expect("temp root");
        let (config, session) = host(&root);
        let bare = Bare;
        let stack = Stack {
            session: &session,
            config: &config,
            cancel: Cancel::new(),
            executor: &bare,
            opener: Some(Arc::new(NoBrowser)),
        };

        assert_eq!(
            names(&stack),
            ["bash", "skill", "open"],
            "the frame is where a human is, so it is handed the third tool"
        );
    }

    #[test]
    fn bash_is_described_with_the_timeout_this_run_enforces() {
        let root = TempDir::new().expect("temp root");
        let (config, session) = host(&root);
        let bare = Bare;
        let stack = Stack {
            session: &session,
            config: &config,
            cancel: Cancel::new(),
            executor: &bare,
            opener: None,
        };

        let specs = stack.dials().tool.specs();
        let bash = &specs[0];
        assert_eq!(bash.name, "bash");
        assert!(
            bash.description.contains("still running after 5 s"),
            "the description names the boundary the executor enforces: {}",
            bash.description
        );
    }

    #[test]
    fn the_channel_the_stack_composes_is_the_one_its_agent_holds() {
        let root = TempDir::new().expect("temp root");
        let (config, session) = host(&root);
        let bare = Bare;
        let stack = Stack {
            session: &session,
            config: &config,
            cancel: Cancel::new(),
            executor: &bare,
            opener: None,
        };

        let (agent, events) = stack.agent();
        drop(agent);

        assert!(
            events.recv().is_err(),
            "the agent holds the sender of the channel it was handed, so the run's events travel on it"
        );
    }

    #[test]
    fn a_kept_agent_is_wired_again_for_the_next_prompt() {
        let root = TempDir::new().expect("temp root");
        let (config, session) = host(&root);
        let bare = Bare;
        let stack = Stack {
            session: &session,
            config: &config,
            cancel: Cancel::new(),
            executor: &bare,
            opener: None,
        };

        let (mut agent, first) = stack.agent();
        let next = stack.wire(&mut agent);
        drop(first);
        drop(agent);

        assert!(
            next.recv().is_err(),
            "the channel is the kept agent's, and it is the newest one that travels"
        );
    }
}
