//! Asking the wire for a turn: the one adapter between the run loop and the
//! provider seam, holding a client and the ask a front end built from its config,
//! and putting each turn's own words to the wire as that turn's prompt.
//!
//! Everything the ask does not vary with — the dialect, the served name, the
//! endpoint, the credential, the preamble that is re-sent every turn, whether the
//! model reasons — is settled once, where the config was read, so a turn cannot
//! ask a different wire or a different model from the one the record names.
//!
//! A reply's text is the answer and a failure is the one line safe to print, which
//! is the whole of what the loop asks of a [`Responder`]. A reply that asks for a
//! tool call cannot arrive here yet: the provider seam folds a reply's text and
//! keeps no call, so a batch run of this build answers in words and acts in none
//! (ADR-0016's "not yet"). The loop runs calls for a responder that can name one,
//! and it is that seam and not this module that will grow them.

use std::sync::{Arc, Mutex};

use crate::provider::{Ask, Client, Deltas, Failure};
use crate::records::Usage;
use crate::run::{Reply, Request, Responder};

/// The model, as the loop needs it: one ask put to one wire per turn, with the
/// reply's text handed back as the turn's answer.
#[derive(Clone, Debug)]
pub struct Asking {
    /// Where a request goes: the provider seam, and never a client library.
    client: Client,
    /// The ask every turn is built from, as the config and the preamble left it.
    ask: Ask,
    /// What the asks have spent so far, summed as each reply lands: a `Responder`
    /// is asked through a shared reference, so the total is behind a lock and is
    /// shared by every clone of this value.
    spent: Arc<Mutex<Usage>>,
}

impl Asking {
    /// A run asking `ask` through `client`: an ask with no prompt yet is one a
    /// model can be asked, so the prompt is the only thing a turn supplies.
    pub fn new(client: Client, ask: Ask) -> Asking {
        Asking {
            client,
            ask,
            spent: Arc::new(Mutex::new(Usage::default())),
        }
    }

    /// What the run's asks spent, as much of it as the wire reported: the record
    /// and §15's object both read this one sum rather than counting twice.
    pub fn spent(&self) -> Usage {
        *self
            .spent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The ask one turn is: the settled ask, with this turn's own words as its
    /// prompt.
    fn ask(&self, request: &Request) -> Ask {
        self.ask.clone().prompt(request.prompt())
    }
}

impl Responder for Asking {
    fn respond(&self, request: &Request, deltas: &mut dyn Deltas) -> Result<Reply, Failure> {
        let asked = self.ask(request);
        let answer = self.client.ask(&asked, deltas)?;
        self.spent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .charge(&answer.usage);
        Ok(Reply::answer(answer.text))
    }
}
