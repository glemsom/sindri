//! A turn's commands answered by the script itself, after a delay it chooses:
//! the fixtures a test about a *batch* of calls needs, kept out of `support`
//! because the rest of the suite runs one command at a time.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sindri_core::agent::Agent;
use sindri_core::cancel::Cancel;
use sindri_core::exec::{BashExecutor, ToolResult};
use sindri_core::provider::Turn;

use super::{Harness, ScriptedModel, exited, fixtures};

/// An executor that answers with the script it was given, after a delay that
/// script chooses. A test about the shape of a turn's results must not depend on
/// the order they ran in, and a delay is the only way to say which finishes
/// first without betting on how fast a thread is scheduled.
pub struct Echoing {
    seen: Arc<Mutex<Vec<String>>>,
    delay: Box<dyn Fn(&str) -> Duration + Send + Sync>,
}

impl Echoing {
    /// Named `sharing` rather than `new`: it answers with an executor the caller
    /// shares, so the return type is the point and not a smell.
    pub fn sharing(
        seen: Arc<Mutex<Vec<String>>>,
        delay: impl Fn(&str) -> Duration + Send + Sync + 'static,
    ) -> Arc<dyn BashExecutor> {
        Arc::new(Echoing {
            seen,
            delay: Box::new(delay),
        })
    }
}

impl BashExecutor for Echoing {
    fn execute(&self, script: &str) -> Option<ToolResult> {
        self.seen.lock().expect("seen").push(script.to_string());
        std::thread::sleep((self.delay)(script));
        Some(exited(&format!("{script}\n")))
    }
}

impl Harness {
    /// A harness whose commands echo the script that asked for them, after
    /// `delay` milliseconds, so a turn of several calls can be measured rather
    /// than guessed at.
    pub fn echoing(
        replies: Vec<Turn>,
        delay: impl Fn(&str) -> Duration + Send + Sync + 'static,
        max_turns: u32,
    ) -> Self {
        let (workspace, _session_temp, policy) = fixtures();
        let seen_scripts = Arc::new(Mutex::new(Vec::new()));
        let cancel = Cancel::new();
        let executor = Echoing::sharing(Arc::clone(&seen_scripts), delay);
        let bash = sindri_core::agent::BashTool::new(
            &policy,
            sindri_core::agent::Limits::default(),
            executor,
        );
        let tool = sindri_core::agent::Tools::new(bash, None, None);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let told = Arc::new(Mutex::new(Vec::new()));
        let model = ScriptedModel::new(replies, Arc::clone(&seen), Arc::clone(&told));
        let (sender, events) = std::sync::mpsc::channel();
        let mut agent = Agent::new(Box::new(model), tool, max_turns, false);
        agent.set_events(sender);
        agent.set_cancel(cancel);
        Harness {
            agent,
            seen,
            seen_scripts,
            thinking: told,
            events,
            _workspace: workspace,
        }
    }
}
