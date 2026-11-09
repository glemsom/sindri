//! The one adapter between the run loop and the provider seam. A reply that asks for a
//! tool call cannot arrive here yet: the seam folds a reply's text and keeps no call, so
//! a run of this build answers in words and acts in none.

use std::sync::{Arc, Mutex};

use crate::provider::{Ask, Client, Deltas, Failure};
use crate::records::Usage;
use crate::run::{Reply, Request, Responder};

#[derive(Clone, Debug)]
pub struct Asking {
    client: Client,
    ask: Ask,
    spent: Arc<Mutex<Usage>>,
}

impl Asking {
    pub fn new(client: Client, ask: Ask) -> Asking {
        Asking {
            client,
            ask,
            spent: Arc::new(Mutex::new(Usage::default())),
        }
    }

    pub fn spent(&self) -> Usage {
        *self
            .spent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

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
        Ok(Reply::answer(answer.text).spent(answer.usage))
    }
}
