//! The skill the reader asked for, as the frame holds it: the request, the wait
//! for the front end to answer it, and the prompt written behind it. A load is a
//! task of its own, so the turn it precedes waits for the body to be in the
//! conversation the turn will be sent with.

use super::App;
use crate::driver::Invoked;

/// Where `/skill` has got to, in the same three states as [`Renew`]: the frame
/// marks the request, the loop hands it to the front end once, and the turn a
/// reader armed behind it starts only when an answer arrives either way.
///
/// [`Renew`]: super::Renew
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Loading {
    #[default]
    Idle,
    /// The name the reader asked for, not yet handed over.
    Asked(String),
    /// Handed over, and the answer has not come back.
    Waiting,
}

impl App {
    /// The skill the frame is waiting on, once: what the loop hands the front end
    /// as a task of its own. Two states would not do — the frame marks the
    /// request and the loop hands it over once — so it is asked, then waited on,
    /// until the answer arrives either way.
    pub fn take_load(&mut self) -> Option<String> {
        match std::mem::take(&mut self.loading) {
            Loading::Asked(skill) => {
                self.loading = Loading::Waiting;
                Some(skill)
            }
            other => {
                self.loading = other;
                None
            }
        }
    }

    /// What the front end made of the skill the frame asked for. The load
    /// itself arrives as the event every other load arrives as — one row and one
    /// chip, whether a human asked or the model did — so a repeat and a refusal
    /// are the two answers left to say here.
    pub fn loaded(&mut self, loaded: Result<Invoked, String>) {
        self.loading = Loading::Idle;
        match loaded {
            // The row and the rail's chip came with the event the front end
            // sent before this answer; nothing is left to say.
            Ok(Invoked::Loaded(_) | Invoked::Already(_)) => {}
            Err(error) => {
                self.note_error(error);
                // A prompt waiting on a load that did not happen is not run: it
                // would be a turn the skill the reader asked for was not in.
                self.armed = None;
            }
        }
    }

    /// Whether the frame is waiting on a load: a turn the reader armed does not
    /// start until the answer lands.
    pub fn is_loading(&self) -> bool {
        self.loading != Loading::Idle
    }
}
